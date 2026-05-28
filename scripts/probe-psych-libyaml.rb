#!/usr/bin/env ruby
# frozen_string_literal: true

require "date"
require "json"
require "psych"

EXPECTED_RUBY = "2.6.10"
EXPECTED_PSYCH = "3.1.0"
EXPECTED_LIBYAML = "0.2.1"

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
    YAML
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
  }
].freeze

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

def probe_case(entry)
  value = Psych.load(entry[:yaml])
  {
    id: entry[:id],
    record: entry[:record],
    status: "ok",
    summary: summarize(value)
  }
rescue Psych::SyntaxError => error
  {
    id: entry[:id],
    record: entry[:record],
    status: "error",
    error_class: error.class.name,
    error: error.problem || error.message.lines.first.to_s.strip
  }
end

payload = {
  probe: "psych-libyaml-divergence",
  ruby: RUBY_VERSION,
  psych: Psych::VERSION,
  libyaml: actual_libyaml,
  cases: CASES.map { |entry| probe_case(entry) }
}

puts JSON.pretty_generate(payload)
