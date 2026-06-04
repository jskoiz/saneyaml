fn main() -> saneyaml::Result<()> {
    docker_compose_edit()?;
    kubernetes_label_edit()?;
    github_actions_edit()?;
    Ok(())
}

fn docker_compose_edit() -> saneyaml::Result<()> {
    let source = "\
# service stack
services:
  web:
    image: nginx:1.25
    ports:
      - \"80:80\"
";
    let mut editor = saneyaml::edit(source)?;
    editor
        .set(
            saneyaml::ConfigPath::new([
                saneyaml::PathSegment::from("services"),
                saneyaml::PathSegment::from("web"),
                saneyaml::PathSegment::from("image"),
            ]),
            "nginx:1.27",
        )?
        .push(
            saneyaml::ConfigPath::new([
                saneyaml::PathSegment::from("services"),
                saneyaml::PathSegment::from("web"),
                saneyaml::PathSegment::from("ports"),
            ]),
            "8080:80",
        )?;

    let edited = editor.finish()?;
    assert!(edited.contains("# service stack"));
    assert!(edited.contains("image: nginx:1.27"));
    assert!(edited.contains("- 8080:80"));
    Ok(())
}

fn kubernetes_label_edit() -> saneyaml::Result<()> {
    let source = "\
metadata:
  labels:
    app.kubernetes.io/name: web
";
    let mut editor = saneyaml::edit(source)?;
    editor.set(
        saneyaml::ConfigPath::json_pointer("/metadata/labels/app.kubernetes.io~1name")?,
        "api",
    )?;

    let edited = editor.finish()?;
    assert!(edited.contains("app.kubernetes.io/name: api"));
    Ok(())
}

fn github_actions_edit() -> saneyaml::Result<()> {
    let source = "\
jobs:
  test:
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
";
    let mut editor = saneyaml::edit(source)?;
    editor.set(
        saneyaml::ConfigPath::new([
            saneyaml::PathSegment::from("jobs"),
            saneyaml::PathSegment::from("test"),
            saneyaml::PathSegment::from("steps"),
            saneyaml::PathSegment::from(0usize),
            saneyaml::PathSegment::from("uses"),
        ]),
        "actions/checkout@v6",
    )?;

    let edited = editor.finish()?;
    assert!(edited.contains("uses: actions/checkout@v6"));
    assert!(edited.contains("- run: cargo test"));
    Ok(())
}
