use apollo_federation::composition::{compose, CompositionError};
use apollo_federation::subgraph::{Subgraph, Initial, Expanded, Upgraded};
use apollo_compiler::Schema;

/// Helper to print SDL for snapshot assertions.
fn print_sdl(schema: &Schema) -> String {
    let mut schema = schema.clone();
    schema.types.sort_keys();
    schema.directive_definitions.sort_keys();
    schema.to_string()
}

/// Helper to build a subgraph through initial → expanded → upgraded.
fn build_subgraph(name: &str, url: &str, sdl: &str) -> Subgraph<Upgraded> {
    Subgraph::parse(name, url, sdl)
        .expect("parse")
        .expand_links()
        .expect("expand_links")
        .assume_upgraded()
}

/// Compose two typical subgraphs.
#[test]
fn compose_typical_two_subgraphs() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            type Query { foo: Foo }
            type Foo { id: ID! name: String }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            type Foo { id: ID! description: String }
        "#,
    );
    let supergraph = compose(vec![s1, s2]).expect("Should compose");
    let sdl = print_sdl(supergraph.schema());
    assert!(sdl.contains("type Foo"));
    assert!(sdl.contains("description: String"));
    assert!(sdl.contains("name: String"));
}

/// Compose with conflicting type definitions (edge case).
#[test]
fn compose_conflicting_types() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            type Query { foo: Foo }
            type Foo { id: ID! name: String }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            type Foo { id: Int! name: String }
        "#,
    );
    let result = compose(vec![s1, s2]);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| format!("{:?}", e).contains("merge_subgraphs failed")));
}

/// Compose with enum merging (edge case).
#[test]
fn compose_enum_merging() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            enum Color { RED GREEN }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            enum Color { GREEN BLUE }
        "#,
    );
    let supergraph = compose(vec![s1, s2]).expect("Should compose");
    let sdl = print_sdl(supergraph.schema());
    assert!(sdl.contains("enum Color"));
    assert!(sdl.contains("RED"));
    assert!(sdl.contains("GREEN"));
    assert!(sdl.contains("BLUE"));
}

/// Compose with missing Query type (edge case).
#[test]
fn compose_missing_query_type() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            type Foo { id: ID! }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            type Bar { id: ID! }
        "#,
    );
    let result = compose(vec![s1, s2]);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| format!("{:?}", e).contains("not a valid federation schema")));
}

/// Compose with custom directives (edge case).
#[test]
fn compose_with_custom_directives() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            directive @foo on FIELD
            type Query { a: String @foo }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            directive @bar on FIELD
            type Query { b: String @bar }
        "#,
    );
    let supergraph = compose(vec![s1, s2]).expect("Should compose");
    let sdl = print_sdl(supergraph.schema());
    assert!(sdl.contains("directive @foo"));
    assert!(sdl.contains("directive @bar"));
}

/// Compose with empty subgraph list (edge case).
#[test]
fn compose_empty_subgraph_list() {
    let result = compose(vec![]);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| format!("{:?}", e).contains("not a valid federation schema")));
}

/// Compose with duplicate type names (edge case).
#[test]
fn compose_duplicate_type_names() {
    let s1 = build_subgraph(
        "A",
        "https://a",
        r#"
            type Query { foo: Foo }
            type Foo { id: ID! }
        "#,
    );
    let s2 = build_subgraph(
        "B",
        "https://b",
        r#"
            type Query { foo: Foo }
            type Foo { id: ID! }
        "#,
    );
    let supergraph = compose(vec![s1, s2]).expect("Should compose");
    let sdl = print_sdl(supergraph.schema());
    assert_eq!(sdl.matches("type Foo").count(), 1);
}