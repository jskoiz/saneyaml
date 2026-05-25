#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::Value;

fuzz_target!(|input: &[u8]| {
    let Ok(mut value) = yaml::from_slice::<Value>(input) else {
        return;
    };

    if let Err(error) = value.apply_merge() {
        assert!(!error.to_string().is_empty());
        assert_eq!(error.location(), None);
    }
});
