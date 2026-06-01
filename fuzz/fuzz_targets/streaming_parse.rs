#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Ok(input_str) = std::str::from_utf8(input) else {
        return;
    };

    let batch_events = yaml::parse_events(input_str);
    let streamed_events = yaml::EventStream::from_str(input_str)
        .and_then(|stream| stream.collect::<yaml::Result<Vec<_>>>());
    assert_eq!(
        streamed_events, batch_events,
        "pull event stream diverged from parse_events"
    );

    let batch_documents = yaml::parse_documents(input_str);
    let streamed_documents = yaml::DocumentStream::from_str(input_str)
        .and_then(|stream| stream.collect::<yaml::Result<Vec<_>>>());
    assert_eq!(
        streamed_documents, batch_documents,
        "pull document stream diverged from parse_documents"
    );
});
