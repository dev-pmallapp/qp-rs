//! Tests for KernelConfig builder and functionality.

use qf::kernel::{Kernel, KernelConfig};

#[test]
fn kernel_config_builder() {
    let config = KernelConfig::builder()
        .name("TestKernel")
        .max_active(32)
        .max_event_pools(5)
        .max_tick_rate(10)
        .counter_sizes(2, 2)
        .version(800)
        .build();

    assert_eq!(config.name, "TestKernel");
    assert_eq!(config.max_active, 32);
    assert_eq!(config.max_event_pools, 5);
    assert_eq!(config.max_tick_rate, 10);
    assert_eq!(config.event_queue_ctr_size, 2);
    assert_eq!(config.time_event_ctr_size, 2);
    assert_eq!(config.version, 800);
}

#[test]
fn kernel_config_default() {
    let config = KernelConfig::default();

    assert_eq!(config.name, "QP");
    assert_eq!(config.max_active, 16);
    assert_eq!(config.max_event_pools, 3);
    assert_eq!(config.max_tick_rate, 4);
    assert_eq!(config.version, 740);
}

#[test]
fn kernel_with_custom_config() {
    let config = KernelConfig::builder()
        .name("CustomKernel")
        .max_active(64)
        .build();

    let kernel = Kernel::with_config(config).build();

    assert_eq!(kernel.config().name, "CustomKernel");
    assert_eq!(kernel.config().max_active, 64);
}

#[test]
fn kernel_idle_callback() {
    use std::sync::{Arc, Mutex};

    let idle_called = Arc::new(Mutex::new(false));
    let idle_clone = idle_called.clone();

    fn idle_callback() {
        // This would be set if we could access the Arc from here
        // For now, just verify it compiles
    }

    let config = KernelConfig::builder()
        .idle_callback(idle_callback)
        .build();

    assert!(config.idle_callback.is_some());
}

#[cfg(feature = "qs")]
#[test]
fn kernel_config_to_target_info() {
    let config = KernelConfig::builder()
        .name("QSTest")
        .max_active(24)
        .max_event_pools(4)
        .max_tick_rate(8)
        .counter_sizes(4, 4)
        .version(750)
        .build();

    let target_info = config.to_target_info();

    assert_eq!(target_info.max_active, 24);
    assert_eq!(target_info.max_event_pools, 4);
    assert_eq!(target_info.max_tick_rate, 8);
    assert_eq!(target_info.equeue_ctr_size, 4);
    assert_eq!(target_info.time_evt_ctr_size, 4);
    assert_eq!(target_info.version, 750);
}
