use super::*;

#[test]
fn sse_multibyte_boundaries_multiline_and_truncated_events() {
    let wire = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\r\n\r\n";
    let mut sse = stream::Sse::default();
    let mut events = vec![];
    for b in wire.as_bytes() {
        events.extend(sse.push(&[*b]).unwrap());
    }
    assert_eq!(events.len(), 1);
    assert!(sse.complete());
    assert!(events[0].contains("你好"));
    sse.push(b"data: unfinished").unwrap();
    assert!(!sse.complete());
}
