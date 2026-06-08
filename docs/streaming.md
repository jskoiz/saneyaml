# Streaming

Pull-based iterators that process YAML without holding every parsed document at
once. Use them for large multi-document streams; for small configs,
[`from_str`](getting-started.md) is simpler.

> Snippets elide the enclosing function; assume a function returning
> `saneyaml::Result<()>`.

## Two levels

| Stream | Yields | Use when |
|---|---|---|
| `DocumentStream` | one semantic `Node` per document | You want documents one at a time, with merge expansion and schema applied. |
| `EventStream` | low-level parser events | You want raw structure — scalar style, flow vs block, anchors, the literal `<<` — without building a tree. |

Both construct from `&str`, `&[u8]`, or a reader, and both are plain `Iterator`s.

## Documents one at a time

```rust
for doc in saneyaml::stream::DocumentStream::from_str(stream)? {
    let node = doc?; // saneyaml::Node
    // handle one document, then it can be dropped before the next is parsed
}
```

## Raw events

```rust
use saneyaml::Event;

for event in saneyaml::stream::EventStream::from_str(source)? {
    match event? {
        Event::Scalar { .. }       => { /* value, with its style + tag */ }
        Event::MappingStart { .. } => { /* … */ }
        Event::Alias { .. }        => { /* a raw *alias, not expanded */ }
        _ => {}
    }
}
```

Events expose what the semantic tree throws away: scalar quote style, block vs
flow collection style, anchors/aliases as distinct events, tags, and document
directives. Aliases are **not** expanded here — that's what makes events the
right tool for preserving or analyzing the original document.

## What "bounded memory" means

Streaming bounds the *retained parsed representation*: `DocumentStream` keeps one
document live at a time instead of a whole `Vec`. The **source bytes are still
fully buffered** — these are synchronous pull APIs over an in-memory input, not
constant-memory async readers.

The memory win is real on multi-document streams (the working set stays flat as
the stream grows) and negligible on a single large document, where there's
nothing to reclaim mid-parse. The [benchmarks](BENCHMARKS.md) quantify both.

## Need it all at once?

`parse_documents` / `parse_events` are the all-or-error collectors over the same
parser, returning a `Vec`. Reach for the streams only when you want to act on
items as they arrive or cap retained documents.
