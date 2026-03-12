use tfgrid_sdk_rust::{
    DEV_NETWORK, GridClientConfig, GridError, MAIN_NETWORK, QA_NETWORK, TEST_NETWORK,
};

#[test]
fn named_presets_expose_expected_primary_endpoints() {
    let dev = GridClientConfig::devnet();
    let qa = GridClientConfig::qanet();
    let test = GridClientConfig::testnet();
    let main = GridClientConfig::mainnet();

    assert_eq!(dev.network, DEV_NETWORK);
    assert_eq!(dev.substrate_url, "wss://tfchain.dev.grid.tf/ws");
    assert_eq!(dev.grid_proxy_url, "https://gridproxy.dev.grid.tf");
    assert_eq!(dev.graphql_url, "https://graphql.dev.grid.tf/graphql");
    assert_eq!(dev.relay_url, "wss://relay.dev.grid.tf");
    assert_eq!(dev.kyc_url, "https://kyc.dev.grid.tf");
    assert_eq!(dev.substrate_urls.len(), 2);
    assert_eq!(dev.grid_proxy_urls.len(), 3);

    assert_eq!(qa.network, QA_NETWORK);
    assert_eq!(qa.substrate_url, "wss://tfchain.qa.grid.tf/ws");
    assert_eq!(qa.grid_proxy_url, "https://gridproxy.qa.grid.tf");
    assert_eq!(qa.graphql_url, "https://graphql.qa.grid.tf/graphql");
    assert_eq!(qa.relay_url, "wss://relay.qa.grid.tf");

    assert_eq!(test.network, TEST_NETWORK);
    assert_eq!(test.substrate_url, "wss://tfchain.test.grid.tf/ws");

    assert_eq!(main.network, MAIN_NETWORK);
    assert_eq!(main.substrate_url, "wss://tfchain.grid.tf/ws");
    assert_eq!(main.grid_proxy_url, "https://gridproxy.grid.tf");
    assert_eq!(main.graphql_url, "https://graphql.grid.tf/graphql");
    assert_eq!(main.relay_url, "wss://relay.grid.tf");
    assert_eq!(main.substrate_urls.len(), 6);
}

#[test]
fn from_network_accepts_aliases_and_rejects_unknown_names() {
    let qa = GridClientConfig::from_network("qanet").expect("qa alias");
    let test = GridClientConfig::from_network("TESTNET").expect("test alias");
    let main = GridClientConfig::from_network(" main ").expect("main alias");

    assert_eq!(qa.network, QA_NETWORK);
    assert_eq!(test.network, TEST_NETWORK);
    assert_eq!(main.network, MAIN_NETWORK);

    let err = GridClientConfig::from_network("staging").expect_err("unknown network");
    assert!(matches!(err, GridError::Validation(_)));
    assert_eq!(
        err.to_string(),
        "invalid input: unsupported network preset `staging`"
    );
}

#[test]
fn plural_endpoint_overrides_become_primary_when_not_pinned() {
    let config = GridClientConfig::builder()
        .network(MAIN_NETWORK)
        .grid_proxy_urls(vec![
            "https://proxy-a.example.test/".to_string(),
            " https://proxy-b.example.test/ ".to_string(),
        ])
        .relay_urls(vec![
            "wss://relay-a.example.test/".to_string(),
            "wss://relay-b.example.test/".to_string(),
        ])
        .build();

    assert_eq!(config.grid_proxy_url, "https://proxy-a.example.test");
    assert_eq!(
        config.grid_proxy_urls,
        vec![
            "https://proxy-a.example.test".to_string(),
            "https://proxy-b.example.test".to_string(),
        ]
    );
    assert_eq!(config.relay_url, "wss://relay-a.example.test");
    assert_eq!(
        config.relay_urls,
        vec![
            "wss://relay-a.example.test".to_string(),
            "wss://relay-b.example.test".to_string(),
        ]
    );
}

#[test]
fn explicit_primary_endpoint_wins_over_plural_overrides() {
    let config = GridClientConfig::builder()
        .network(MAIN_NETWORK)
        .grid_proxy_urls(vec![
            "https://proxy-a.example.test".to_string(),
            "https://proxy-b.example.test".to_string(),
        ])
        .grid_proxy_url(" https://proxy-primary.example.test/ ")
        .substrate_urls(vec![
            "wss://substrate-a.example.test/ws".to_string(),
            "wss://substrate-b.example.test/ws".to_string(),
        ])
        .substrate_url("wss://substrate-primary.example.test/ws/")
        .build();

    assert_eq!(config.grid_proxy_url, "https://proxy-primary.example.test");
    assert_eq!(
        config.grid_proxy_urls,
        vec![
            "https://proxy-a.example.test".to_string(),
            "https://proxy-b.example.test".to_string(),
        ]
    );
    assert_eq!(
        config.substrate_url,
        "wss://substrate-primary.example.test/ws"
    );
    assert_eq!(
        config.substrate_urls,
        vec![
            "wss://substrate-a.example.test/ws".to_string(),
            "wss://substrate-b.example.test/ws".to_string(),
        ]
    );
}
