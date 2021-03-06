/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use fnv::FnvHashSet;
use graphql_ir::{Program, ValidationResult};
use graphql_transforms::{
    apply_fragment_arguments, client_extensions, flatten, generate_id_field, generate_typename,
    handle_field_transform, inline_fragments, mask, remove_base_fragments, skip_client_extensions,
    skip_redundant_nodes, skip_split_operation, skip_unreachable_node, split_module_import,
    transform_connections, transform_defer_stream, transform_match, ConnectionInterface,
};
use interner::StringKey;

pub struct Programs<'schema> {
    pub source: Program<'schema>,
    pub reader: Program<'schema>,
    pub normalization: Program<'schema>,
    pub operation_text: Program<'schema>,
    pub typegen: Program<'schema>,
}

pub fn apply_transforms<'schema, TConnectionInterface: ConnectionInterface>(
    program: Program<'schema>,
    base_fragment_names: &FnvHashSet<StringKey>,
    connection_interface: &TConnectionInterface,
) -> ValidationResult<Programs<'schema>> {
    // common
    //  |- reader
    //  |- operation
    //     |- normalization
    //     |- operation_text
    let common_program = apply_common_transforms(&program, connection_interface)?;
    let reader_program = apply_reader_transforms(&common_program, base_fragment_names);
    let operation_program = apply_operation_transforms(&common_program)?;
    let normalization_program = apply_normalization_transforms(&operation_program);
    let operation_text_program = apply_operation_text_transforms(&operation_program);
    let typegen_program = apply_typegen_transforms(&program, base_fragment_names);

    Ok(Programs {
        source: program,
        reader: reader_program,
        normalization: normalization_program,
        operation_text: operation_text_program,
        typegen: typegen_program,
    })
}

/// Applies transforms that apply to every output.
fn apply_common_transforms<'schema, TConnectionInterface: ConnectionInterface>(
    program: &Program<'schema>,
    connection_interface: &TConnectionInterface,
) -> ValidationResult<Program<'schema>> {
    // JS compiler
    // - DisallowIdAsAlias
    // + ConnectionTransform
    // - RelayDirectiveTransform
    // + MaskTransform
    // + MatchTransform
    // - RefetchableFragmentTransform
    // + DeferStreamTransform

    let program = transform_connections(program, connection_interface);
    let program = mask(&program);
    let program = transform_match(&program)?;
    transform_defer_stream(&program)
}

/// Applies transforms only for generated reader code.
/// Corresponds to the "fragment transforms" in the JS compiler.
fn apply_reader_transforms<'schema>(
    program: &Program<'schema>,
    base_fragment_names: &FnvHashSet<StringKey>,
) -> Program<'schema> {
    // JS compiler
    // + ClientExtensionsTransform
    // + FieldHandleTransform
    // - InlineDataFragmentTransform
    // + FlattenTransform, flattenAbstractTypes: true
    // - SkipRedundantNodesTransform
    let program = handle_field_transform(&program);
    let program = remove_base_fragments(&program, base_fragment_names);
    let program = flatten(&program, true);
    client_extensions(&program)
}

/// Applies transforms that apply to all operation artifacts.
/// Corresponds to the "query transforms" in the JS compiler.
fn apply_operation_transforms<'schema>(
    program: &Program<'schema>,
) -> ValidationResult<Program<'schema>> {
    // JS compiler
    // + SplitModuleImportTransform
    // - ValidateUnusedVariablesTransform
    // + ApplyFragmentArgumentTransform
    // - ValidateGlobalVariablesTransform
    // + GenerateIDFieldTransform
    // - TestOperationTransform
    let program = split_module_import(&program);
    let program = apply_fragment_arguments(&program)?;
    let program = generate_id_field(&program);

    Ok(program)
}

/// After the operation transforms, this applies further transforms that only
/// apply to the generated normalization code.
///
/// Corresponds to the "codegen transforms" in the JS compiler
fn apply_normalization_transforms<'schema>(program: &Program<'schema>) -> Program<'schema> {
    // JS compiler
    // + SkipUnreachableNodeTransform
    // + InlineFragmentsTransform
    // + ClientExtensionsTransform
    // + FlattenTransform, flattenAbstractTypes: true
    // + SkipRedundantNodesTransform
    // + GenerateTypeNameTransform
    // - ValidateServerOnlyDirectivesTransform

    let program = skip_unreachable_node(&program);
    let program = inline_fragments(&program);
    let program = flatten(&program, true);
    let program = skip_redundant_nodes(&program);
    let program = client_extensions(&program);
    generate_typename(&program)
}

/// After the operation transforms, this applies further transforms that only
/// apply to the printed operation text.
///
/// Corresponds to the "print transforms" in the JS compiler
fn apply_operation_text_transforms<'schema>(program: &Program<'schema>) -> Program<'schema> {
    // JS compiler
    // + SkipSplitOperationTransform
    // - ClientExtensionsTransform
    // + SkipClientExtensionsTransform
    // + SkipUnreachableNodeTransform
    // + FlattenTransform, flattenAbstractTypes: false
    // + GenerateTypeNameTransform
    // - SkipHandleFieldTransform
    // - FilterDirectivesTransform
    // - SkipUnusedVariablesTransform
    // - ValidateRequiredArgumentsTransform

    let program = skip_split_operation(&program);
    let program = skip_client_extensions(&program);
    let program = skip_unreachable_node(&program);
    let program = flatten(&program, false);
    generate_typename(&program)
}

fn apply_typegen_transforms<'schema>(
    program: &Program<'schema>,
    base_fragment_names: &FnvHashSet<StringKey>,
) -> Program<'schema> {
    // JS compiler
    // - RelayDirectiveTransform,
    // - MaskTransform
    // - MatchTransform
    // + FlattenTransform, flattenAbstractTypes: false
    // - RefetchableFragmentTransform,

    let program = remove_base_fragments(&program, base_fragment_names);
    flatten(&program, false)
}
