use pavis::proxy::service::test_exports::{should_log_upstream_config, should_sample_pool_key};

#[test]
fn test_should_sample_pool_key() {
    // Default sample rate is 16
    let mut trues = 0;
    for _ in 0..32 {
        if should_sample_pool_key() {
            trues += 1;
        }
    }
    // Should be sampled twice in 32 calls with rate 16
    assert_eq!(trues, 2);
}

#[test]
fn test_should_log_upstream_config() {
    // It uses a counter and is_multiple_of(256)
    // We need to call it 256 times to get past the first check if it's not the first call.
    // Since it's a global atomic, we don't know the current value.

    let mut saw_true = false;
    for i in 0..1000 {
        if should_log_upstream_config(i as u64) {
            saw_true = true;
            // Once we get a true, the next call with same hash should be false
            assert!(!should_log_upstream_config(i as u64));
            break;
        }
    }
    assert!(saw_true);
}
