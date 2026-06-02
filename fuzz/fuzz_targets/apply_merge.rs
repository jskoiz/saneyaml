#![no_main]

use libfuzzer_sys::fuzz_target;
use saneyaml::Value;

fuzz_target!(|input: &[u8]| {
    assert_apply_merge_invariants(input);
});

fn assert_apply_merge_invariants(input: &[u8]) {
    let Ok(mut value) = saneyaml::from_slice::<Value>(input) else {
        return;
    };

    match value.apply_merge() {
        Ok(()) => {
            value
                .apply_merge()
                .expect("repeated apply_merge should keep succeeding");

            if should_compare_shared_merge_subset(input) {
                assert_matches_serde_yaml(input);
            }
        }
        Err(error) => {
            assert!(!error.to_string().is_empty());
            assert_eq!(error.location(), None);
        }
    }
}

fn should_compare_shared_merge_subset(input: &[u8]) -> bool {
    let Ok(input) = std::str::from_utf8(input) else {
        return false;
    };
    if input.contains('!') || input.contains('%') {
        return false;
    }

    input.contains("<<")
}

fn assert_matches_serde_yaml(input: &[u8]) {
    let input = std::str::from_utf8(input).expect("shared merge subset is UTF-8");
    let Ok(reference_unmerged) = serde_yaml::from_str::<serde_yaml::Value>(input) else {
        return;
    };
    let Ok(mut value) = saneyaml::to_value(reference_unmerged.clone()) else {
        return;
    };
    let mut reference = reference_unmerged;
    match (value.apply_merge(), reference.apply_merge()) {
        (Ok(()), Ok(())) => {
            let reference =
                saneyaml::to_value(reference).expect("serde_yaml value converts to saneyaml::Value");
            assert!(value.equivalent(&reference));
        }
        (Err(error), Err(reference_error)) => {
            assert!(!error.to_string().is_empty());
            assert_eq!(error.location(), None);
            assert!(reference_error.location().is_none());
        }
        (Ok(()), Err(reference_error)) => {
            panic!("yaml applied merge but serde_yaml rejected it: {reference_error}");
        }
        (Err(error), Ok(())) => {
            panic!("yaml rejected merge but serde_yaml applied it: {error}");
        }
    }
}
