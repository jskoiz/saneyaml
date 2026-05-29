#!/usr/bin/env ruby
# frozen_string_literal: true

require "date"
require "digest"
require "json"
require "psych"

EXPECTED_RUBY = "2.6.10"
EXPECTED_PSYCH = "3.1.0"
EXPECTED_LIBYAML = "0.2.1"
ROOT = File.expand_path("..", __dir__)

def fixture_yaml(path)
  File.read(File.join(ROOT, path))
end

actual_libyaml = Psych.libyaml_version.join(".")
unless RUBY_VERSION == EXPECTED_RUBY &&
       Psych::VERSION == EXPECTED_PSYCH &&
       actual_libyaml == EXPECTED_LIBYAML
  warn "expected Ruby #{EXPECTED_RUBY}, Psych #{EXPECTED_PSYCH}, libyaml #{EXPECTED_LIBYAML}; " \
       "got Ruby #{RUBY_VERSION}, Psych #{Psych::VERSION}, libyaml #{actual_libyaml}"
  exit 1
end

CASES = [
  {
    id: "legacy-scalar-resolution",
    record: "tests/fixtures/divergences/records/legacy-scalar-resolution.toml",
    yaml: <<~YAML
      on: yes
      timestamp: 2026-05-24
      octal: 0123
      sexagesimal: 1:20
      special_float: .inf
    YAML
  },
  {
    id: "rw-github-actions-on-key",
    record: "tests/fixtures/divergences/records/rw-github-actions-on-key.toml",
    yaml: <<~YAML
      on:
        push:
          branches: [main]
    YAML
  },
  {
    id: "merge-keys",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: <<~YAML
      base: &base
        image: app:v1
        replicas: 2
      service:
        <<: *base
        image: app:v2
      first: &first
        shared: first
        image: app:first
        retries: 3
      second: &second
        shared: second
        image: app:second
        timeout: 10
      list_service:
        <<: [*first, *second]
      explicit_service:
        <<: [*first, *second]
        shared: explicit
        timeout: explicit
      tagged_service:
        !!merge <<: *base
        image: app:tagged
      canonical_service:
        !<tag:yaml.org,2002:merge> <<: *base
        image: app:canonical
      string_service:
        !!str <<: literal
        image: app:string
      custom_service:
        !Thing <<: literal
        image: app:custom
      scalar_merge:
        <<: scalar
        keep: value
      quoted_scalar_merge:
        '<<': literal
        keep: value
      tagged_scalar_merge:
        !!merge <<: literal
        keep: value
      sequence_scalar_merge:
        <<: [scalar]
        keep: value
      repeated_merge:
        <<: *first
        <<: *second
        keep: value
      repeated_tagged_merge:
        !!merge <<: *first
        !<tag:yaml.org,2002:merge> <<: *second
        keep: value
    YAML
  },
  {
    id: "merge-nested-list-precedence",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: <<~YAML
      base: &base {a: 1, shared: base}
      mid: &mid {<<: *base, b: 2, shared: mid}
      other: &other {shared: other, c: 3}
      target:
        <<: [*mid, *other]
        shared: target
    YAML
  },
  {
    id: "merge-duplicate-local-key-policy",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: <<~YAML
      base: &base {a: 1}
      target:
        <<: *base
        a: local1
        a: local2
    YAML
  },
  {
    id: "merge-cross-document-anchor-reset",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    mode: :stream,
    yaml: <<~YAML
      ---
      base: &base {a: 1}
      ---
      merged:
        <<: *base
    YAML
  },
  {
    id: "merge-mixed-invalid-list-payload",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: <<~YAML
      base: &base {a: 1}
      target:
        <<: [*base, scalar]
        keep: value
    YAML
  },
  {
    id: "alias-graph-identity",
    record: "tests/fixtures/divergences/records/alias-graph-identity.toml",
    yaml: <<~YAML
      base: &base
        count: 1
      a: *base
      b: *base
    YAML
  },
  {
    id: "alias-redefinition-identity",
    record: "tests/fixtures/divergences/records/alias-graph-identity.toml",
    yaml: <<~YAML
      first: &item one
      b: *item
      second: &item two
      d: *item
    YAML
  },
  {
    id: "alias-recursive-identity",
    record: "tests/fixtures/divergences/records/alias-graph-identity.toml",
    yaml: "root: &root [*root]\n"
  },
  {
    id: "duplicate-scalar-keys",
    record: "tests/fixtures/divergences/records/duplicate-scalar-keys.toml",
    yaml: <<~YAML
      name: first
      name: second
    YAML
  },
  {
    id: "explicit-core-tags",
    record: "tests/fixtures/divergences/records/explicit-core-tags.toml",
    yaml: <<~YAML
      binary: !!binary SGVsbG8=
      int: !!int 0x7B
      octal: !!int 0123
      sexagesimal: !!int 1:20
      int_tag_float: !!int 1:20.5
      timestamp: !!timestamp 2026-05-24
      float: !!float .inf
      sexagesimal_float: !!float 1:20:30.5
      string_null: !!str null
      string_bool: !!str true
      bool_on: !!bool ON
      bool_false: !!bool false
      null_value: !!null null
    YAML
  },
  {
    id: "yaml11-collection-tags",
    record: "tests/fixtures/divergences/records/yaml11-collection-tags.toml",
    yaml: <<~YAML
      set: !!set
        ? alpha
        ? beta
      omap: !!omap
        - first: 1
        - second: 2
      pairs: !!pairs
        - repeat: 1
        - repeat: 2
    YAML
  },
  {
    id: "yaml11-set-non-null-payload",
    record: "tests/fixtures/divergences/records/yaml11-collection-tags.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/set-rejects-non-null-values.yaml")
  },
  {
    id: "yaml11-omap-non-singleton-entry",
    record: "tests/fixtures/divergences/records/yaml11-collection-tags.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/omap-rejects-non-singleton-entry.yaml")
  },
  {
    id: "yaml11-pairs-scalar-entry",
    record: "tests/fixtures/divergences/records/yaml11-collection-tags.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/pairs-rejects-scalar-entry.yaml")
  },
  {
    id: "yaml11-core-structural-tags",
    record: "tests/fixtures/divergences/records/yaml11-core-structural-tags.toml",
    yaml: <<~YAML
      %YAML 1.1
      %TAG !yaml! tag:yaml.org,2002:
      ---
      short_seq: !!seq [1, 2]
      canonical_seq: !<tag:yaml.org,2002:seq> [a, b]
      resolved_seq: !yaml!seq [left, right]
      short_map: !!map {a: 1, b: 2}
      canonical_map: !<tag:yaml.org,2002:map> {c: 3}
      resolved_map: !yaml!map {d: 4}
      value_key: !!value =
      value_mapping:
        ? !!value =
        : value
    YAML
  },
  {
    id: "core-structural-tags",
    record: "tests/fixtures/divergences/records/yaml11-core-structural-tags.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/core-structural-tags.yaml")
  },
  {
    id: "legacy-merge-edge-recovery",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/legacy-merge-edge-recovery.yaml")
  },
  {
    id: "explicit-merge-tags",
    record: "tests/fixtures/divergences/records/merge-keys.toml",
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/explicit-merge-tags.yaml")
  },
  {
    id: "lossless-merge-graph",
    record: "tests/fixtures/divergences/records/alias-graph-identity.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/lossless-merge-graph.yaml")
  },
  {
    id: "lossless-recursive-graph",
    record: "tests/fixtures/divergences/records/alias-graph-identity.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml11-conformance/lossless-recursive-graph.yaml")
  },
  {
    id: "null-like-string-targets",
    record: "tests/fixtures/divergences/records/null-like-string-targets.toml",
    yaml: <<~YAML
      empty:
      tilde: ~
      null_word: null
      quoted: "null"
    YAML
  },
  {
    id: "numeric-key-identity",
    record: "tests/fixtures/divergences/records/numeric-key-identity.toml",
    yaml: <<~YAML
      1: integer
      1.0: float
      "1": string
    YAML
  },
  {
    id: "tab-token-separation",
    record: "tests/fixtures/divergences/records/tab-token-separation.toml",
    yaml: "key:\tvalue\n"
  },
  {
    id: "adjacent-flow-mapping-scalars",
    record: "tests/fixtures/divergences/records/adjacent-flow-mapping-scalars.toml",
    yaml: "{foo: bar:baz}\n"
  },
  {
    id: "multiline-quoted-flow-key",
    record: "tests/fixtures/divergences/records/multiline-quoted-flow-key.toml",
    yaml: <<~YAML
      {
        "first
        second": value
      }
    YAML
  },
  {
    id: "raw-event-directives",
    record: "tests/fixtures/divergences/records/raw-event-directives.toml",
    mode: :events,
    yaml: <<~YAML
      %YAML 1.1
      %TAG !e! tag:example.com,2026:
      --- !e!Thing &root {a: 1}
    YAML
  },
  {
    id: "raw-event-document-markers",
    record: "tests/fixtures/divergences/records/raw-event-document-markers.toml",
    mode: :events,
    yaml: <<~YAML
      ---
      alpha: 1
      ...
      --- beta
    YAML
  },
  {
    id: "yaml-version-directive-schema",
    record: "tests/fixtures/divergences/records/yaml-version-directive-schema.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml-test-suite/data/BEC7/in.yaml")
  },
  {
    id: "tag-directive-scope-and-undeclared-handles",
    record: "tests/fixtures/divergences/records/tag-directive-scope-and-undeclared-handles.toml",
    mode: :events,
    yaml: "!h!Thing value\n"
  },
  {
    id: "document-start-inline-node",
    record: "tests/fixtures/divergences/records/document-start-inline-node.toml",
    mode: :events,
    yaml: "--- &root !Thing {a: 1}\n"
  },
  {
    id: "document-start-block-scalars",
    record: "tests/fixtures/divergences/records/document-start-block-scalars.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml-test-suite/data/W4TN/in.yaml")
  },
  {
    id: "bare-document-streams",
    record: "tests/fixtures/divergences/records/bare-document-streams.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml-test-suite/data/M7A3/in.yaml")
  },
  {
    id: "directive-looking-flow-content",
    record: "tests/fixtures/divergences/records/directive-looking-flow-content.toml",
    mode: :events,
    yaml: fixture_yaml("tests/fixtures/yaml-test-suite/data/UT92/in.yaml")
  }
].freeze

class EventSummary < Psych::Handler
  attr_reader :events

  def initialize
    @events = []
  end

  def start_stream(encoding)
    push("start_stream", encoding: encoding)
  end

  def end_stream
    push("end_stream")
  end

  def start_document(version, tag_directives, implicit)
    push("start_document", version: version, tag_directives: tag_directives, implicit: implicit)
  end

  def end_document(implicit)
    push("end_document", implicit: implicit)
  end

  def scalar(value, anchor, tag, plain, quoted, style)
    push(
      "scalar",
      value: value,
      anchor: anchor,
      tag: tag,
      plain_implicit: plain,
      quoted_implicit: quoted,
      style: style
    )
  end

  def start_sequence(anchor, tag, implicit, style)
    push("start_sequence", anchor: anchor, tag: tag, implicit: implicit, style: style)
  end

  def end_sequence
    push("end_sequence")
  end

  def start_mapping(anchor, tag, implicit, style)
    push("start_mapping", anchor: anchor, tag: tag, implicit: implicit, style: style)
  end

  def end_mapping
    push("end_mapping")
  end

  def alias(anchor)
    push("alias", anchor: anchor)
  end

  private

  def push(event, **fields)
    @events << fields.merge(event: event)
  end
end

def scalar_summary(value)
  summary = { class: value.class.name }
  summary[:value] =
    case value
    when Float
      if value.infinite?
        value.infinite?.positive? ? "Infinity" : "-Infinity"
      elsif value.nan?
        "NaN"
      else
        value.to_s
      end
    when Time, Date, DateTime
      value.iso8601
    when NilClass
      nil
    when TrueClass, FalseClass
      value
    else
      value.to_s
    end
  summary
end

def summarize(value)
  case value
  when Hash
    {
      class: value.class.name,
      entries: value.map do |key, item|
        { key: scalar_summary(key), value: summarize(item) }
      end
    }
  when Array
    { class: value.class.name, items: value.map { |item| summarize(item) } }
  else
    scalar_summary(value)
  end
end

def with_input_metadata(entry, result)
  result.merge(
    input_sha256: Digest::SHA256.hexdigest(entry[:yaml]),
    input_bytes: entry[:yaml].bytesize
  )
end

def error_fields(error)
  fields = {
    error_class: error.class.name,
    error: error.respond_to?(:problem) && error.problem ? error.problem : error.message.lines.first.to_s.strip
  }
  fields[:context] = error.context if error.respond_to?(:context) && error.context
  fields[:line] = error.line if error.respond_to?(:line) && error.line
  fields[:column] = error.column if error.respond_to?(:column) && error.column
  fields
end

def probe_alias_graph_identity(entry)
  shared = Psych.load(entry[:yaml])
  shared_alias_identity = shared["a"].object_id == shared["b"].object_id
  shared["a"]["count"] = 2

  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: {
      shared_alias_identity: shared_alias_identity,
      mutation_visible_in_b: shared["b"]["count"]
    }
  })
rescue Psych::Exception => error
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "error",
  }.merge(error_fields(error)))
end

def probe_recursive_alias_identity(entry)
  recursive = Psych.load(entry[:yaml])
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: {
      recursive_identity: recursive["root"].object_id == recursive["root"][0].object_id
    }
  })
rescue Psych::Exception => error
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "error",
  }.merge(error_fields(error)))
end

def probe_case(entry)
  return probe_alias_graph_identity(entry) if entry[:id] == "alias-graph-identity"
  return probe_recursive_alias_identity(entry) if entry[:id] == "alias-recursive-identity"
  return probe_events(entry) if entry[:mode] == :events
  return probe_stream(entry) if entry[:mode] == :stream

  value = Psych.load(entry[:yaml])
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: summarize(value)
  })
rescue Psych::Exception => error
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "error",
  }.merge(error_fields(error)))
end

def probe_stream(entry)
  docs = Psych.load_stream(entry[:yaml]).to_a
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: {
      document_count: docs.length,
      documents: docs.map { |doc| summarize(doc) }
    }
  })
rescue Psych::Exception => error
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "error",
  }.merge(error_fields(error)))
end

def probe_events(entry)
  handler = EventSummary.new
  Psych::Parser.new(handler).parse(entry[:yaml])
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: {
      event_count: handler.events.length,
      events: handler.events
    }
  })
rescue Psych::Exception => error
  with_input_metadata(entry, {
    id: entry[:id],
    record: entry[:record],
    status: "error",
  }.merge(error_fields(error)))
end

payload = {
  probe: "psych-libyaml-divergence",
  ruby: RUBY_VERSION,
  psych: Psych::VERSION,
  libyaml: actual_libyaml,
  cases: CASES.map { |entry| probe_case(entry) }
}

puts JSON.pretty_generate(payload)
