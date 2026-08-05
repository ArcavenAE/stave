//! Operation registry over the curated GraphQL library (`stave-api`)
//! plus classification of ad-hoc documents (`stave api --query`).
//!
//! Two write-guard-relevant facts are decided here:
//!
//! * A **curated** operation's type comes from its registry entry
//!   (checked at xtask time against the document itself).
//! * An **ad-hoc** document is parsed: any mutation or subscription
//!   operation anywhere in the document marks it mutating. Parsing
//!   failures are fatal — an unparseable document never reaches the
//!   wire, so nothing can hide from the guard.

use graphql_parser::query::{Definition, OperationDefinition};
pub use stave_api::{OpType, OperationDoc};

use crate::error::{Result, StaveError};

/// Look up a curated operation, with a repair-friendly error naming
/// the discovery command.
pub fn find(name: &str) -> Result<&'static OperationDoc> {
    stave_api::find(name).ok_or_else(|| StaveError::UnknownOperation(name.to_string()))
}

/// All curated operations, in stable order.
pub fn all() -> &'static [OperationDoc] {
    stave_api::OPERATIONS
}

/// Metadata recovered from an ad-hoc GraphQL document.
#[derive(Clone, Debug)]
pub struct DocumentMeta {
    /// The first operation's name, if the document names one.
    pub operation_name: Option<String>,
    /// True when any operation in the document is a mutation or
    /// subscription — drives the write-guard.
    pub is_mutating: bool,
}

/// Parse an ad-hoc document and classify it for the write-guard.
pub fn classify_document(source: &str) -> Result<DocumentMeta> {
    let doc = graphql_parser::parse_query::<String>(source)
        .map_err(|e| StaveError::Document(format!("parse error: {e}")))?;

    let mut operation_name: Option<String> = None;
    let mut is_mutating = false;
    for def in &doc.definitions {
        if let Definition::Operation(op) = def {
            match op {
                OperationDefinition::Mutation(m) => {
                    is_mutating = true;
                    if operation_name.is_none() {
                        operation_name = m.name.clone();
                    }
                }
                OperationDefinition::Subscription(s) => {
                    is_mutating = true;
                    if operation_name.is_none() {
                        operation_name = s.name.clone();
                    }
                }
                OperationDefinition::Query(q) => {
                    if operation_name.is_none() {
                        operation_name = q.name.clone();
                    }
                }
                OperationDefinition::SelectionSet(_) => {}
            }
        }
    }
    Ok(DocumentMeta {
        operation_name,
        is_mutating,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_known_operation() {
        let op = find("list_issues").expect("registered");
        assert_eq!(op.root_field, "issuesV2");
        assert_eq!(op.op_type, OpType::Query);
    }

    #[test]
    fn find_unknown_operation_errors() {
        let err = find("nope").expect_err("unknown");
        assert!(matches!(err, StaveError::UnknownOperation(_)));
    }

    #[test]
    fn classify_query_is_not_mutating() {
        let meta = classify_document("query Foo { issuesV2 { nodes { id } } }").expect("parse");
        assert!(!meta.is_mutating);
        assert_eq!(meta.operation_name.as_deref(), Some("Foo"));
    }

    #[test]
    fn classify_bare_selection_set_is_not_mutating() {
        let meta = classify_document("{ issuesV2 { nodes { id } } }").expect("parse");
        assert!(!meta.is_mutating);
        assert_eq!(meta.operation_name, None);
    }

    #[test]
    fn classify_mutation_is_mutating() {
        let meta =
            classify_document("mutation Fix { resolveIssue(id: \"x\") { id } }").expect("parse");
        assert!(meta.is_mutating);
        assert_eq!(meta.operation_name.as_deref(), Some("Fix"));
    }

    #[test]
    fn classify_mixed_document_is_mutating() {
        // A query hiding a mutation in the same document must still trip
        // the guard.
        let meta = classify_document(
            "query A { issuesV2 { nodes { id } } }\nmutation B { deleteReport(id: \"x\") }",
        )
        .expect("parse");
        assert!(meta.is_mutating);
    }

    #[test]
    fn classify_unparseable_errors() {
        let err = classify_document("query { unbalanced").expect_err("parse error");
        assert!(matches!(err, StaveError::Document(_)));
    }

    #[test]
    fn every_curated_document_parses_and_matches_registry_type() {
        for op in all() {
            let meta = classify_document(op.document)
                .unwrap_or_else(|e| panic!("curated op {} unparseable: {e}", op.name));
            assert_eq!(
                meta.is_mutating,
                op.op_type == OpType::Mutation,
                "registry op_type disagrees with document for {}",
                op.name
            );
        }
    }
}
