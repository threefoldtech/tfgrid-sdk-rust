use tfgrid_sdk_rust::{
    FullNetworkTarget, NetworkTarget, NodePlacement, NodeRequirements, VmDeployment,
    VmLightDeployment, VmLightSpec, VmSpec, VolumeMountSpec,
};

#[test]
fn vm_light_builder_exposes_configurable_public_api() {
    let request = VmLightDeployment::builder()
        .auto_with(
            NodeRequirements::builder()
                .min_cru(2)
                .min_memory_bytes(2 * 1024 * 1024 * 1024)
                .min_rootfs_bytes(20 * 1024 * 1024 * 1024)
                .build(),
        )
        .create_network(
            tfgrid_sdk_rust::NetworkLightSpec::builder()
                .name("integration-net")
                .subnet("10.77.2.0/24")
                .mycelium_key(vec![1, 2, 3])
                .build(),
        )
        .vm(VmLightSpec::builder()
            .name("integration-vm")
            .cpu(2)
            .memory_bytes(2 * 1024 * 1024 * 1024)
            .rootfs_size_bytes(20 * 1024 * 1024 * 1024)
            .entrypoint("/bin/sh -c")
            .env("SSH_KEY", "ssh-ed25519 test")
            .mount("data", "/data")
            .volume(
                VolumeMountSpec::new("data", 5 * 1024 * 1024 * 1024, "/data")
                    .description("attached data disk"),
            )
            .corex(true)
            .gpu("0000:29:00.0")
            .mycelium_seed(vec![7, 8, 9])
            .build())
        .build();

    match request.placement {
        NodePlacement::Auto(requirements) => {
            assert_eq!(requirements.min_cru, 2);
            assert_eq!(requirements.min_memory_bytes, 2 * 1024 * 1024 * 1024);
            assert_eq!(requirements.min_rootfs_bytes, 20 * 1024 * 1024 * 1024);
        }
        other => panic!("unexpected placement: {other:?}"),
    }

    match request.network {
        NetworkTarget::Create(network) => {
            assert_eq!(network.name.as_deref(), Some("integration-net"));
            assert_eq!(network.subnet.as_deref(), Some("10.77.2.0/24"));
            assert_eq!(network.mycelium_key, Some(vec![1, 2, 3]));
        }
        other => panic!("unexpected network target: {other:?}"),
    }

    assert_eq!(request.vm.name.as_deref(), Some("integration-vm"));
    assert_eq!(request.vm.cpu, 2);
    assert_eq!(request.vm.entrypoint, "/bin/sh -c");
    assert_eq!(
        request.vm.env.get("SSH_KEY").map(String::as_str),
        Some("ssh-ed25519 test")
    );
    assert_eq!(request.vm.mounts.len(), 1);
    assert_eq!(request.vm.mounts[0].name, "data");
    assert_eq!(request.vm.mounts[0].mountpoint, "/data");
    assert_eq!(request.vm.volumes.len(), 1);
    assert_eq!(request.vm.volumes[0].description, "attached data disk");
    assert!(request.vm.corex);
    assert_eq!(request.vm.gpu, vec!["0000:29:00.0".to_string()]);
    assert_eq!(request.vm.mycelium_seed, Some(vec![7, 8, 9]));
}

#[test]
fn vm_builder_supports_existing_network_and_public_flags() {
    let request = VmDeployment::builder()
        .fixed_node(11, 22)
        .existing_network("existing-net", "10.88.2.5")
        .vm(VmSpec::builder()
            .name("public-vm")
            .cpu(4)
            .memory_bytes(4 * 1024 * 1024 * 1024)
            .rootfs_size_bytes(40 * 1024 * 1024 * 1024)
            .public_ipv4(true)
            .public_ipv6(true)
            .planetary(true)
            .env("ROLE", "gateway")
            .volume(VolumeMountSpec::new(
                "cache",
                10 * 1024 * 1024 * 1024,
                "/var/cache",
            ))
            .build())
        .build();

    match request.placement {
        NodePlacement::Fixed {
            node_id,
            node_twin_id,
        } => {
            assert_eq!(node_id, 11);
            assert_eq!(node_twin_id, 22);
        }
        other => panic!("unexpected placement: {other:?}"),
    }

    match request.network {
        FullNetworkTarget::Existing(network) => {
            assert_eq!(network.name, "existing-net");
            assert_eq!(network.ip, "10.88.2.5");
        }
        other => panic!("unexpected network target: {other:?}"),
    }

    assert_eq!(request.vm.name.as_deref(), Some("public-vm"));
    assert_eq!(request.vm.cpu, 4);
    assert!(request.vm.public_ipv4);
    assert!(request.vm.public_ipv6);
    assert!(request.vm.planetary);
    assert_eq!(
        request.vm.env.get("ROLE").map(String::as_str),
        Some("gateway")
    );
    assert_eq!(request.vm.volumes.len(), 1);
    assert_eq!(request.vm.volumes[0].name, "cache");
}
