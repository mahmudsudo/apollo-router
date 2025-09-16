mod satisfiability;

use std::vec;
pub use crate::{composition::satisfiability::validate_satisfiability,connectors::validation::validate,error::CompositionError};
pub use crate::schema::{schema_upgrader::upgrade_subgraphs_if_necessary,ValidFederationSchema};
use crate::subgraph::{typestate::{Expanded,Initial,Subgraph,Validated,Upgraded},ValidSubgraph};
pub use crate::supergraph::{Merged,Satisfiable,Supergraph};

/// Options for composition configuration.
pub struct CompositionOptions {
    pub run_satisfiability: bool,
    pub max_validation_subgraph_paths: Option<usize>,
}

impl Default for CompositionOptions {
    fn default() -> Self {
        Self {
            run_satisfiability: true,
            max_validation_subgraph_paths: None,
        }
    }
}

/// Result type for composition operations.
pub type CompositionResult<T> = Result<T, Vec<CompositionError>>;

pub fn compose(subgraphs: Vec<Subgraph<Initial>>) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    compose_with_options(subgraphs, CompositionOptions::default())
}

pub fn compose_with_options(
    subgraphs: Vec<Subgraph<Initial>>,
    options: CompositionOptions,
) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    let expanded_subgraphs = expand_subgraphs(subgraphs)?;
    let upgraded_subgraphs = upgrade_subgraphs_if_necessary(expanded_subgraphs)?;
    let validated_subgraphs = validate_subgraphs(upgraded_subgraphs)?;

    pre_merge_validations(&validated_subgraphs)?;
    let supergraph = merge_subgraphs(validated_subgraphs)?;
    post_merge_validations(&supergraph)?;

    if options.run_satisfiability {
        validate_satisfiability(supergraph)
    } else {
        // Skip satifiability validation if disabled .
        Ok(Supergraph::<Satisfiable>::new(ValidFederationSchema::new(supergraph.state.schema().clone()).unwrap(), supergraph.hints().clone()))
    }
}

/// Apollo Federation allow subgraphs to specify partial schemas (i.e. "import" directives through
/// `@link`). This function will update subgraph schemas with all missing federation definitions.
pub fn expand_subgraphs(
    subgraphs: Vec<Subgraph<Initial>>,
) -> Result<Vec<Subgraph<Expanded>>, Vec<CompositionError>> {
    let mut errors: Vec<CompositionError> = vec![];
    let expanded: Vec<Subgraph<Expanded>> = subgraphs
        .into_iter()
        .map(|s| s.expand_links())
        .filter_map(|r| r.map_err(|e| errors.push(e.into())).ok())
        .collect();
    if errors.is_empty() {
        Ok(expanded)
    } else {
        Err(errors)
    }
}

/// Validate subgraph schemas to ensure they satisfy Apollo Federation requirements (e.g. whether
/// `@key` specifies valid `FieldSet`s etc).
pub fn validate_subgraphs(
    subgraphs: Vec<Subgraph<Upgraded>>,
) -> Result<Vec<Subgraph<Validated>>, Vec<CompositionError>> {
    let mut errors: Vec<CompositionError> = vec![];
    let validated: Vec<Subgraph<Validated>> = subgraphs
        .into_iter()
        .map(|s| s.validate())
        .filter_map(|r| r.map_err(|e| errors.push(e.into())).ok())
        .collect();
    if errors.is_empty() {
        Ok(validated)
    } else {
        Err(errors)
    }
}


/// Perform validations that require information about all available subgraphs.
pub fn pre_merge_validations(
    subgraphs: &[Subgraph<Validated>],
) -> Result<(), Vec<CompositionError>> {
    let mut errors = Vec::new();
    //  validate that all subgraphs are Federation 2 subgraphs.
    subgraphs.iter().for_each(|s| {
        if !is_fed2_subgraph(s) {
            errors.push(CompositionError::InternalError {
                message: "Subgraph is not a Federation 2 subgraph".to_string(),
            });
        }
    });

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_fed2_subgraph(subgraph: &Subgraph<Validated>) -> bool {
    subgraph.metadata().is_fed_2_schema()
}

pub fn merge_subgraphs(
    subgraphs: Vec<Subgraph<Validated>>,
) -> Result<Supergraph<Merged>, Vec<CompositionError>> {
    //  convert to ValidSubgraph format expected by the merge module.
    let valid_subgraphs = subgraphs.into_iter().map(|s| ValidSubgraph {
        name: s.name.clone(),
        url: s.url.clone(),
        schema: s.validated_schema().schema().clone(),
    })
        .collect::<Vec<ValidSubgraph>>();

    // Create references for the merge function.
    let subgraph_refs = valid_subgraphs.iter().collect::<Vec<&ValidSubgraph>>();
    //  call the  merge implementation of merge_subgraphs
    let merge_result = crate::merge::merge_subgraphs(subgraph_refs)
        .map_err(|e| vec![CompositionError::InternalError {
            message: format!("merge_subgraphs failed: {:?}", e),
        }])?;

    // finally, convert the result to expected type.
    let supergraph = Supergraph::<Merged>::new(merge_result.schema);
    Ok(supergraph)
}

pub fn post_merge_validations(
    supergraph: &Supergraph<Merged>,
) -> Result<(), Vec<CompositionError>> {
    let mut errors = Vec::new();
    // TODO : move print_sdl to be part of the output of compose 
   println!("Post-merge supergraph SDL:\n{}", print_sdl(supergraph));

    // validate that the resulting supergraph is a valid federation schema.
    if let Err(e) = ValidFederationSchema::new(supergraph.schema().clone()) {
        errors.push(CompositionError::InternalError {
            message: format!("Resulting supergraph is not a valid federation schema: {:?}", e),
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn print_sdl(schema:  &Supergraph<Merged>) -> String {
    let   schema = schema.state.schema().clone();
    schema.types.clone().sort_keys();
    schema.directive_definitions.clone().sort_keys();
    schema.to_string()
}