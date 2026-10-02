use ruff_python_ast as ast;
use ruff_python_ast::Stmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::class::{
    ClassMemberKind, any_member_declaration, iter_super_class,
};
use ruff_python_semantic::analyze::visibility::is_abstract;
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::delete_stmt;
use crate::{AlwaysFixableViolation, Fix};

/// ## What it does
/// Checks for redundant definitions of `__str__` or `__repr__` in stubs.
///
/// ## Why is this bad?
/// Defining `__str__` or `__repr__` in a stub is almost always redundant,
/// as the signatures are almost always identical to those of the default
/// equivalent, `object.__str__` and `object.__repr__`, respectively.
///
/// ## Example
///
/// ```pyi
/// class Foo:
///     def __repr__(self) -> str: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271", category = Category::Style)]
pub(crate) struct StrOrReprDefinedInStub {
    name: String,
}

impl AlwaysFixableViolation for StrOrReprDefinedInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StrOrReprDefinedInStub { name } = self;
        format!("Defining `{name}` in a stub is almost always redundant")
    }

    fn fix_title(&self) -> String {
        let StrOrReprDefinedInStub { name } = self;
        format!("Remove definition of `{name}`")
    }
}

/// PYI029
pub(crate) fn str_or_repr_defined_in_stub(checker: &Checker, stmt: &Stmt) {
    let Stmt::FunctionDef(ast::StmtFunctionDef {
        name,
        decorator_list,
        returns,
        parameters,
        ..
    }) = stmt
    else {
        return;
    };

    let Some(returns) = returns else {
        return;
    };

    if !matches!(name.as_str(), "__str__" | "__repr__") {
        return;
    }

    let ScopeKind::Class(class_def) = checker.semantic().current_scope().kind else {
        return;
    };

    // It is a violation only if the method signature matches that of `object.__str__`
    // or `object.__repr__` exactly and the method is not decorated as abstract.
    if !parameters.kwonlyargs.is_empty()
        || (parameters.args.len() + parameters.posonlyargs.len()) > 1
    {
        return;
    }

    if is_abstract(decorator_list, checker.semantic()) {
        return;
    }

    if !checker.semantic().match_builtin_expr(returns, "str") {
        return;
    }

    // Implementing an abstract method is required, so the definition isn't redundant.
    if overrides_abstract_method(class_def, name, checker.semantic()) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        StrOrReprDefinedInStub {
            name: name.to_string(),
        },
        stmt.identifier(),
    );
    let stmt = checker.semantic().current_statement();
    let parent = checker.semantic().current_statement_parent();
    let edit = delete_stmt(stmt, parent, checker.locator(), checker.indexer());
    diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
        checker.semantic().current_statement_parent_id(),
    )));
}

/// Returns `true` if the nearest superclass of `class_def` that declares `name` declares it as an
/// abstract method.
///
/// Only superclasses defined earlier in the same file are resolved.
fn overrides_abstract_method(
    class_def: &ast::StmtClassDef,
    name: &str,
    semantic: &SemanticModel,
) -> bool {
    iter_super_class(class_def, semantic)
        .skip(1)
        .find_map(|super_class| {
            let mut is_abstract_method = None;
            any_member_declaration(super_class, |declaration| {
                let ClassMemberKind::FunctionDef(function_def) = declaration.kind() else {
                    return false;
                };
                if function_def.name.as_str() != name {
                    return false;
                }
                is_abstract_method = Some(is_abstract(&function_def.decorator_list, semantic));
                true
            });
            is_abstract_method
        })
        .unwrap_or(false)
}
