#![allow(non_snake_case)]

use saneyaml::{Event, ScalarStyle, parse_events};

fn event_source(input: &str, span: saneyaml::Span) -> &str {
    &input[span.start..span.end]
}

fn assert_scalar_at(
    input: &str,
    events: &[Event],
    value: &str,
    style: ScalarStyle,
    line: usize,
    column: usize,
    source: &str,
) {
    assert!(
        events.iter().any(|event| {
            matches!(
                event,
                Event::Scalar {
                    value: actual,
                    style: actual_style,
                    span,
                    ..
                } if actual == value
                    && *actual_style == style
                    && (span.line, span.column) == (line, column)
                    && event_source(input, *span) == source
            )
        }),
        "missing scalar {value:?} at {line}:{column}"
    );
}

#[test]
fn rw_events_docker_compose__anchors_aliases_and_literal_merge_keys_are_raw() {
    let input = include_str!("fixtures/real-world/docker-compose/compose-anchors.yaml");
    let events = parse_events(input).expect("compose events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| {
                    anchor.name == "service-defaults" && anchor.span.line == 3
                })
        )
    }));

    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some((anchor.name.as_str(), anchor.span.line)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        aliases,
        [("service-defaults", 12), ("service-defaults", 17)]
    );

    let merge_key_count = events
        .iter()
        .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "<<"))
        .count();
    assert_eq!(merge_key_count, 2);

    let mapping_starts = events
        .iter()
        .filter(|event| matches!(event, Event::MappingStart { .. }))
        .count();
    assert!(
        mapping_starts < 10,
        "raw events should not emit expanded mappings for aliases"
    );
}

#[test]
fn rw_events_docker_compose__adapted_spec_fragments_keep_anchor_graph_raw() {
    let input =
        include_str!("fixtures/real-world/docker-compose/adapted-compose-spec-fragments.yaml");
    let events = parse_events(input).expect("Compose spec fragment events");

    let anchors = events
        .iter()
        .filter_map(|event| match event {
            Event::MappingStart { meta, .. } => meta
                .anchor
                .as_ref()
                .map(|anchor| (anchor.name.as_str(), anchor.span.line)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(anchors, [("default-environment", 1), ("keys", 4)]);

    let aliases = events
        .iter()
        .filter_map(|event| match event {
            Event::Alias { anchor } => Some((anchor.name.as_str(), anchor.span.line)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, [("default-environment", 10), ("keys", 10)]);

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                span,
                ..
            } if value == "<<" && span.line == 10
        )
    }));
}

#[test]
fn rw_events_ansible__vault_and_unsafe_tags_preserve_tags_and_styles() {
    let input = include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml");
    let events = parse_events(input).expect("ansible events");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                meta,
                ..
            } if value.contains("$ANSIBLE_VAULT;1.1;AES256")
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag == saneyaml::Tag::new("vault") && tag.span.line == 4
                })
        )
    }));

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::DoubleQuoted,
                meta,
                ..
            } if value == "{{ literal_must_not_render }}"
                && meta.tag.as_ref().is_some_and(|tag| {
                    tag.tag == saneyaml::Tag::new("unsafe") && tag.span.line == 7
                })
        )
    }));

    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                meta,
                span,
            } if value.contains("PASSWORD={{ db_password }}")
                && meta.tag.is_none()
                && span.line == 12
        )
    }));
}

#[test]
fn rw_events_common_configs__explicit_document_start_is_accepted() {
    for (name, fixture) in [
        (
            "github-actions",
            include_str!("fixtures/real-world/github-actions/minimal-ci.yaml"),
        ),
        (
            "docker-compose",
            include_str!("fixtures/real-world/docker-compose/compose.yaml"),
        ),
        ("helm", include_str!("fixtures/real-world/helm/values.yaml")),
        (
            "helm-chart",
            include_str!("fixtures/real-world/helm/Chart.yaml"),
        ),
        (
            "openapi",
            include_str!("fixtures/real-world/openapi/petstore-fragment.yaml"),
        ),
        (
            "wrangler",
            include_str!("fixtures/real-world/cloudflare/wrangler.yaml"),
        ),
        (
            "kubernetes",
            include_str!("fixtures/real-world/kubernetes/deployment.yaml"),
        ),
        (
            "ansible",
            include_str!("fixtures/real-world/ansible/playbook.yaml"),
        ),
    ] {
        let input = format!("---\n{fixture}");
        let events = parse_events(&input)
            .unwrap_or_else(|error| panic!("{name} explicit document start should parse: {error}"));
        assert!(
            matches!(
                events.get(1),
                Some(Event::DocumentStart { explicit: true, .. })
            ),
            "{name} should report explicit document start"
        );
    }
}

#[test]
fn rw_events_helm__chart_metadata_dependency_spans_and_styles() {
    let input = include_str!("fixtures/real-world/helm/Chart.yaml");
    let events = parse_events(input).expect("Helm Chart.yaml events");

    assert_scalar_at(input, &events, "1.2.3", ScalarStyle::Plain, 5, 10, "1.2.3");
    assert_scalar_at(
        input,
        &events,
        "artifacthub.io/category",
        ScalarStyle::Plain,
        13,
        3,
        "artifacthub.io/category",
    );
    assert_scalar_at(
        input,
        &events,
        "false",
        ScalarStyle::DoubleQuoted,
        14,
        43,
        "\"false\"",
    );
    assert_scalar_at(
        input,
        &events,
        "oci://registry-1.docker.io/bitnamicharts",
        ScalarStyle::Plain,
        21,
        17,
        "oci://registry-1.docker.io/bitnamicharts",
    );
    assert_scalar_at(
        input,
        &events,
        "~20.1.0",
        ScalarStyle::DoubleQuoted,
        26,
        14,
        "\"~20.1.0\"",
    );
    assert_scalar_at(
        input,
        &events,
        "import-values",
        ScalarStyle::Plain,
        28,
        5,
        "import-values",
    );
}

#[test]
fn rw_events_kubernetes__explicit_stream_terminator_is_preserved() {
    let input = format!(
        "{}\n...\n",
        include_str!("fixtures/real-world/kubernetes/multi-doc.yaml")
    );
    let events = parse_events(&input).expect("terminated Kubernetes stream events");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        1
    );
}

#[test]
fn rw_events_kubernetes__helm_rendered_stream_boundaries_and_styles() {
    let input = include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml");
    let events = parse_events(input).expect("Helm-rendered Kubernetes stream events");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { explicit: true, .. }))
            .count(),
        5
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { explicit: true, .. }))
            .count(),
        1
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "null"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value.contains("canary: true")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "on"
        )
    }));
}

#[test]
fn rw_events_kubernetes__crd_openapi_stream_boundaries_and_plain_scalars() {
    let input = include_str!("fixtures/real-world/kubernetes/custom-resource-definition.yaml");
    let events = parse_events(input).expect("CRD/OpenAPI Kubernetes stream events");

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { explicit: true, .. }))
            .count(),
        2
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == ".spec.replicas"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "ghcr.io/example/widget:1.0"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "x-kubernetes-list-map-keys"
        )
    }));
}

#[test]
fn rw_events_ansible__explicit_boundaries_preserve_tags_and_styles() {
    let input = format!(
        "---\n{}...\n",
        include_str!("fixtures/real-world/ansible/vault-and-unsafe-tags.yaml")
    );
    let events = parse_events(&input).expect("bounded ansible events");

    assert!(matches!(
        events.get(1),
        Some(Event::DocumentStart { explicit: true, .. })
    ));
    assert!(matches!(
        events.iter().rev().nth(1),
        Some(Event::DocumentEnd { explicit: true, .. })
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                meta,
                ..
            } if value.contains("$ANSIBLE_VAULT;1.1;AES256")
                && meta.tag.as_ref().is_some_and(|tag| tag.tag == saneyaml::Tag::new("vault"))
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::DoubleQuoted,
                meta,
                ..
            } if value == "{{ literal_must_not_render }}"
                && meta.tag.as_ref().is_some_and(|tag| tag.tag == saneyaml::Tag::new("unsafe"))
        )
    }));
}

#[test]
fn rw_events_goal05_cloudformation_symfony_and_ci_metadata() {
    let cloudformation = include_str!("fixtures/real-world/cloudformation/sam-api.yaml");
    let cloudformation_events = parse_events(cloudformation).expect("SAM events");
    for expected_tag in ["Ref", "Sub", "GetAtt"] {
        assert!(
            cloudformation_events.iter().any(|event| {
                matches!(
                    event,
                    Event::Scalar { meta, .. }
                        if meta
                            .tag
                            .as_ref()
                            .is_some_and(|tag| tag.tag == saneyaml::Tag::new(expected_tag))
                )
            }),
            "SAM fixture should preserve !{expected_tag} tag"
        );
    }
    assert!(cloudformation_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                meta,
                ..
            } if value.contains("exports.handler")
                && meta.tag.as_ref().is_some_and(|tag| tag.tag == saneyaml::Tag::new("Sub"))
        )
    }));

    let symfony = include_str!("fixtures/real-world/symfony/services.yaml");
    let symfony_events = parse_events(symfony).expect("Symfony events");
    assert!(symfony_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                meta,
                ..
            } if value == "app.health_check"
                && meta
                    .tag
                    .as_ref()
                    .is_some_and(|tag| tag.tag == saneyaml::Tag::new("tagged_iterator"))
        )
    }));
    assert_scalar_at(
        symfony,
        &symfony_events,
        "@cache.app",
        ScalarStyle::SingleQuoted,
        21,
        15,
        "'@cache.app'",
    );

    let gitlab = include_str!("fixtures/real-world/gitlab-ci/basic-pipeline.yml");
    let gitlab_events = parse_events(gitlab).expect("GitLab CI events");
    assert!(gitlab_events.iter().any(|event| {
        matches!(
            event,
            Event::MappingStart { meta, .. }
                if meta.anchor.as_ref().is_some_and(|anchor| anchor.name == "default_job")
        )
    }));
    assert!(gitlab_events.iter().any(|event| {
        matches!(
            event,
            Event::Alias { anchor } if anchor.name == "default_job"
        )
    }));
    assert!(
        gitlab_events
            .iter()
            .filter(|event| matches!(event, Event::Scalar { value, .. } if value == "<<"))
            .count()
            >= 2
    );

    let circle = include_str!("fixtures/real-world/circleci/config.yml");
    let circle_events = parse_events(circle).expect("CircleCI events");
    assert!(circle_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value.contains("cargo test --locked --test real_world_configs")
        )
    }));
    assert!(circle_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "cargo-v1-{{ checksum \"Cargo.lock\" }}"
        )
    }));

    let azure = include_str!("fixtures/real-world/azure-pipelines/azure-pipelines.yml");
    let azure_events = parse_events(azure).expect("Azure Pipelines events");
    assert!(azure_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Literal,
                ..
            } if value.contains("rustup default $(toolchain)")
        )
    }));
    assert!(azure_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "${{ if eq(variables.isMain, true) }}"
        )
    }));

    let github = include_str!("fixtures/real-world/github-actions/reusable-service-workflow.yaml");
    let github_events = parse_events(github).expect("reusable GitHub Actions events");
    assert!(github_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Folded,
                ..
            } if value.contains("--health-cmd pg_isready")
        )
    }));
    assert!(github_events.iter().any(|event| {
        matches!(
            event,
            Event::Scalar {
                value,
                style: ScalarStyle::Plain,
                ..
            } if value == "${{ inputs.image-tag }}"
        )
    }));
}

#[test]
fn rw_events_common_configs__source_spans_and_styles_are_stable() {
    let github = include_str!("fixtures/real-world/github-actions/matrix-ci.yaml");
    let github_events = parse_events(github).expect("GitHub Actions events");
    assert_scalar_at(github, &github_events, "on", ScalarStyle::Plain, 2, 1, "on");
    assert_scalar_at(
        github,
        &github_events,
        "false",
        ScalarStyle::Plain,
        14,
        19,
        "false",
    );
    assert_scalar_at(
        github,
        &github_events,
        "false",
        ScalarStyle::DoubleQuoted,
        15,
        18,
        "\"false\"",
    );
    assert_scalar_at(
        github,
        &github_events,
        "22",
        ScalarStyle::Plain,
        29,
        28,
        "22",
    );
    assert_scalar_at(
        github,
        &github_events,
        "22",
        ScalarStyle::DoubleQuoted,
        32,
        27,
        "\"22\"",
    );
    assert_scalar_at(
        github,
        &github_events,
        "${{ matrix.coverage == true }}",
        ScalarStyle::Plain,
        44,
        13,
        "${{ matrix.coverage == true }}",
    );

    let wrangler = include_str!("fixtures/real-world/cloudflare/wrangler.yaml");
    let wrangler_events = parse_events(wrangler).expect("Wrangler events");
    assert_scalar_at(
        wrangler,
        &wrangler_events,
        "2026-05-23",
        ScalarStyle::DoubleQuoted,
        4,
        21,
        "\"2026-05-23\"",
    );
    assert_scalar_at(
        wrangler,
        &wrangler_events,
        "nodejs_compat",
        ScalarStyle::Plain,
        5,
        23,
        "nodejs_compat",
    );
    assert_scalar_at(
        wrangler,
        &wrangler_events,
        "example.com/*",
        ScalarStyle::Plain,
        9,
        14,
        "example.com/*",
    );
    assert_scalar_at(
        wrangler,
        &wrangler_events,
        "00000000-0000-0000-0000-000000000000",
        ScalarStyle::Plain,
        14,
        18,
        "00000000-0000-0000-0000-000000000000",
    );

    let ansible = include_str!("fixtures/real-world/ansible/playbook.yaml");
    let ansible_events = parse_events(ansible).expect("Ansible events");
    assert_scalar_at(
        ansible,
        &ansible_events,
        "ansible.builtin.copy",
        ScalarStyle::Plain,
        9,
        7,
        "ansible.builtin.copy",
    );
    assert_scalar_at(
        ansible,
        &ansible_events,
        "name={{ app_name }}\nport={{ app_port }}\n",
        ScalarStyle::Literal,
        11,
        18,
        "|\n          name={{ app_name }}\n          port={{ app_port }}",
    );
    assert_scalar_at(
        ansible,
        &ansible_events,
        "ansible.builtin.service",
        ScalarStyle::Plain,
        15,
        7,
        "ansible.builtin.service",
    );
}
