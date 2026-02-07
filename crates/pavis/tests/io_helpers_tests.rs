use pavis::proxy::service::test_exports::should_sample_pool_key;

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
