//! Cost and overdraft calculations.

use std::{sync::Arc, time::Duration, time::SystemTime, time::UNIX_EPOCH};

use crate::{error::GridError, subi, subi::SubstrateExt};

const M_USD_TO_USD: f64 = 1000.0;
const UNIT_FACTOR: f64 = 10_000_000.0;

/// Rust equivalent of the Go calculator used by tfgrid SDK.
pub struct Calculator {
    substrate_conn: Arc<dyn SubstrateExt + Send + Sync>,
    identity: Option<subi::Identity>,
}

impl Calculator {
    pub fn new(
        substrate_conn: Arc<dyn SubstrateExt + Send + Sync>,
        identity: Option<subi::Identity>,
    ) -> Self {
        Self {
            substrate_conn,
            identity,
        }
    }

    pub fn calculate_cost(
        &self,
        cru: u64,
        mru: u64,
        hru: u64,
        sru: u64,
        public_ip: bool,
        certified: bool,
    ) -> Result<f64, GridError> {
        let pricing_policy = self
            .substrate_conn
            .get_pricing_policy(subi::DEFAULT_PRICING_POLICY_ID)?;

        let cu = calculate_cu(cru, mru);
        let su = calculate_su(hru, sru);
        let mut cost_unit = cu * pricing_policy.cu.value as f64
            + su * pricing_policy.su.value as f64
            + if public_ip {
                pricing_policy.ipu.value as f64
            } else {
                0.0
            };
        if certified {
            cost_unit *= 1.25;
        }
        Ok(unit_to_usd(cost_unit * 24.0 * 30.0))
    }

    pub fn calculate_prices_after_discount(&self, cost: f64) -> Result<(f64, f64), GridError> {
        let pricing_policy = self
            .substrate_conn
            .get_pricing_policy(subi::DEFAULT_PRICING_POLICY_ID)?;

        let mut dedicated = cost * (1.0 - pricing_policy.dedicated_nodes_discount as f64 / 100.0);
        let mut shared = cost;

        if let Some(identity) = &self.identity {
            let balance = self.substrate_conn.get_balance(identity.address())?;
            let balance_tft = unit_to_tft(balance.free);
            let balance_usd = self.tft_to_usd(balance_tft)?;
            let (shared_discount, dedicated_discount) =
                get_applicable_discounts(balance_usd, dedicated, shared);
            dedicated *= 1.0 - dedicated_discount;
            shared *= 1.0 - shared_discount;
        }

        Ok((dedicated, shared))
    }

    pub fn calculate_unique_name_cost(&self) -> Result<f64, GridError> {
        let policy = self
            .substrate_conn
            .get_pricing_policy(subi::DEFAULT_PRICING_POLICY_ID)?;
        let monthly = unit_to_usd(policy.unique_name.value as f64 * 24.0 * 30.0);
        Ok(self.calculate_prices_after_discount(monthly)?.1)
    }

    pub fn calculate_ipv4_cost_per_month(&self) -> Result<f64, GridError> {
        let policy = self
            .substrate_conn
            .get_pricing_policy(subi::DEFAULT_PRICING_POLICY_ID)?;
        Ok(unit_to_usd(policy.ipu.value as f64 * 24.0 * 30.0))
    }

    pub fn calculate_contract_overdue(
        &self,
        contract_id: u64,
        allowance: Duration,
    ) -> Result<i64, GridError> {
        let contract = self.substrate_conn.get_contract(contract_id)?;
        if contract.is_deleted() {
            return Err(GridError::ContractDeleted);
        }

        let payment_state = self
            .substrate_conn
            .get_contract_payment_state(contract_id)?;
        let last_updated = UNIX_EPOCH + Duration::from_secs(payment_state.last_updated_seconds);

        let node = contract
            .contract_type
            .node_id()
            .map(|node_id| self.substrate_conn.get_node(node_id))
            .transpose()?;

        let is_certified_node = node.as_ref().is_some_and(|n| n.certification.is_certified);
        let mut total = calculate_total_overdraft_tft(&payment_state);
        total += self.get_unbilled_amount_in_tft(&contract, is_certified_node)?;
        total +=
            self.calculate_period_cost_tft(last_updated, &contract, node.as_ref(), allowance)?;

        if contract.contract_type.kind() == subi::ContractTypeKind::Rent {
            let extra = self.calculate_total_contracts_overdue_on_node(
                contract.contract_type.rent_contract.node,
                allowance,
            )?;
            total += extra as f64;
        }

        Ok(total.ceil() as i64)
    }

    pub fn tft_to_usd(&self, tft: f64) -> Result<f64, GridError> {
        let rate = self.substrate_conn.get_tft_billing_rate()? as f64;
        Ok(tft * (rate / M_USD_TO_USD))
    }

    pub fn usd_to_tft(&self, usd: f64) -> Result<f64, GridError> {
        let rate = self.substrate_conn.get_tft_billing_rate()? as f64;
        if rate == 0.0 {
            return Err(GridError::validation("zero TFT rate"));
        }
        Ok(usd / (rate / M_USD_TO_USD))
    }

    fn calculate_total_contracts_overdue_on_node(
        &self,
        node_id: u32,
        allowance: Duration,
    ) -> Result<i64, GridError> {
        let mut total = 0i64;
        for contract_id in self.substrate_conn.get_node_contracts(node_id)? {
            total += self.calculate_contract_overdue(contract_id, allowance)?;
        }
        Ok(total)
    }

    fn get_unbilled_amount_in_tft(
        &self,
        contract: &subi::Contract,
        is_certified_node: bool,
    ) -> Result<f64, GridError> {
        if contract.contract_type.kind() == subi::ContractTypeKind::Name {
            return Ok(0.0);
        }
        if contract.contract_type.kind() == subi::ContractTypeKind::Node
            && contract.contract_type.node_contract.public_ips_count == 0
        {
            return Ok(0.0);
        }

        let Some(info) = self
            .substrate_conn
            .get_contract_billing_info(contract.contract_id)
            .ok()
        else {
            return Ok(0.0);
        };

        let mut unbilled_usd = unit_to_usd(info.amount_unbilled as f64);
        if is_certified_node {
            unbilled_usd *= 1.25;
        }
        self.usd_to_tft(unbilled_usd)
    }

    fn calculate_period_cost_tft(
        &self,
        last_updated: SystemTime,
        contract: &subi::Contract,
        node: Option<&subi::Node>,
        allowance: Duration,
    ) -> Result<f64, GridError> {
        let Some(node) = node else {
            return Ok(0.0);
        };

        let elapsed = SystemTime::now()
            .duration_since(last_updated)
            .map_err(|e| GridError::Backend(e.to_string()))?;
        let total_seconds = elapsed.as_secs_f64() + allowance.as_secs_f64();

        let monthly_cost_usd = self.calculate_contract_cost(contract, node)?;
        let monthly_cost_tft = self.usd_to_tft(monthly_cost_usd)?;
        let per_second = monthly_cost_tft / (30.0 * 24.0 * 3600.0);
        Ok(per_second * total_seconds)
    }

    fn calculate_contract_cost(
        &self,
        contract: &subi::Contract,
        node: &subi::Node,
    ) -> Result<f64, GridError> {
        match contract.contract_type.kind() {
            subi::ContractTypeKind::Name => self.calculate_unique_name_cost(),
            subi::ContractTypeKind::Node => {
                let node_contract = &contract.contract_type.node_contract;
                let resources = self
                    .substrate_conn
                    .get_node_contract_resources(contract.contract_id)?;
                let mru = bytes_to_gib(resources.used.mru);
                let hru = bytes_to_gib(resources.used.hru);
                let sru = bytes_to_gib(resources.used.sru);
                let cru = resources.used.cru;
                let is_on_rented_node =
                    matches!(self.substrate_conn.get_node_rent_contract(node.id), Ok(id) if id > 0);

                if is_on_rented_node && node_contract.public_ips_count > 0 {
                    let public_ips_cost = self.calculate_ipv4_cost_per_month()?
                        * node_contract.public_ips_count as f64;
                    let maybe_certified = if node.certification.is_certified {
                        public_ips_cost * 1.25
                    } else {
                        public_ips_cost
                    };
                    Ok(self.calculate_prices_after_discount(maybe_certified)?.1)
                } else {
                    let raw = self.calculate_cost(
                        cru,
                        mru,
                        hru,
                        sru,
                        node_contract.public_ips_count > 0,
                        node.certification.is_certified,
                    )?;
                    Ok(self.calculate_prices_after_discount(raw)?.1)
                }
            }
            subi::ContractTypeKind::Rent => {
                let cost = self.calculate_cost(
                    node.resources.cru,
                    bytes_to_gib(node.resources.mru),
                    bytes_to_gib(node.resources.hru),
                    bytes_to_gib(node.resources.sru),
                    false,
                    node.certification.is_certified,
                )?;
                let dedicated = self.calculate_prices_after_discount(cost)?.0;
                let extra_fee = self.substrate_conn.get_dedicated_node_price(node.id)? as f64;
                Ok(dedicated + extra_fee / M_USD_TO_USD)
            }
        }
    }
}

fn unit_to_usd(units: f64) -> f64 {
    units / UNIT_FACTOR
}

fn unit_to_tft(units: u128) -> f64 {
    units as f64 / UNIT_FACTOR
}

fn calculate_total_overdraft_tft(state: &subi::ContractPaymentState) -> f64 {
    unit_to_tft(state.standard_overdraft + state.additional_overdraft)
}

fn get_applicable_discounts(
    balance_usd: f64,
    dedicated_price: f64,
    shared_price: f64,
) -> (f64, f64) {
    let tiers = [
        (0.0, 0.0),
        (1.5, 20.0),
        (3.0, 30.0),
        (6.0, 40.0),
        (18.0, 60.0),
    ];
    let mut best_shared = 0.0;
    let mut best_dedicated = 0.0;

    for (duration, discount) in tiers {
        if balance_usd > shared_price * duration {
            best_shared = discount;
        }
        if balance_usd > dedicated_price * duration {
            best_dedicated = discount;
        }
    }

    (best_shared / 100.0, best_dedicated / 100.0)
}

fn calculate_su(hru: u64, sru: u64) -> f64 {
    hru as f64 / 1200.0 + sru as f64 / 200.0
}

fn calculate_cu(cru: u64, mru: u64) -> f64 {
    let cu1 = (mru as f64 / 4.0).max(cru as f64 / 2.0);
    let cu2 = (mru as f64 / 8.0).max(cru as f64);
    let cu3 = (mru as f64 / 2.0).max(cru as f64 / 4.0);
    cu1.min(cu2).min(cu3)
}

fn bytes_to_gib(value: u64) -> u64 {
    value / 1024 / 1024 / 1024
}
