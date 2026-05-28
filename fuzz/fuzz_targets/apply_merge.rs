#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::Value;

fuzz_target!(|input: &[u8]| {
    assert_apply_merge_invariants(input);
});

fn assert_apply_merge_invariants(input: &[u8]) {
    let Ok(mut value) = yaml::from_slice::<Value>(input) else {
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

    let Ok(value) = yaml::from_str::<Value>(input) else {
        return false;
    };
    contains_literal_merge_key(&value)
}

fn contains_literal_merge_key(value: &Value) -> bool {
    match value {
        Value::Mapping(mapping) => mapping.iter().any(|(key, value)| {
            key.as_str() == Some("<<")
                || contains_literal_merge_key(key)
                || contains_literal_merge_key(value)
        }),
        Value::Sequence(sequence) => sequence.iter().any(contains_literal_merge_key),
        Value::Tagged(tagged) => contains_literal_merge_key(&tagged.value),
        _ => false,
    }
}

fn assert_matches_serde_yaml(input: &[u8]) {
    let input = std::str::from_utf8(input).expect("shared merge subset is UTF-8");
    let Ok(mut value) = yaml::from_str::<Value>(input) else {
        return;
    };
    let Ok(mut reference) = serde_yaml::from_str::<serde_yaml::Value>(input) else {
        return;
    };
    value
        .apply_merge()
        .expect("yaml applies merge for shared subset");
    reference
        .apply_merge()
        .expect("serde_yaml applies merge for shared subset");
    let reference = yaml::to_value(reference).expect("serde_yaml value converts to yaml::Value");
    assert!(value.equivalent(&reference));
}
