//! Executable single-source ownership policy for semantic keyboard bindings.

use std::{collections::BTreeSet, path::Path};

const REGISTRY_ROOT: &str = "src/ui/shortcut_registry/";
const TERMINAL_TRANSLATION: &str = "src/adapters/terminal/input/translation.rs";
const KEYSTROKE_MODEL: &str = "src/ui/input/keystroke.rs";
const UI_REEXPORTS: &[&str] = &["src/ui/input.rs", "src/ui/mod.rs"];
const KEYBINDING_PROJECTION_OWNERS: &[&str] = &[
    "src/ui/settings.rs",
    "src/adapters/terminal/settings.rs",
    "src/ui/app.rs",
    "src/ui/app/view.rs",
    "src/ui/app/view_frame.rs",
];

pub(crate) fn required_owner_findings(root: &Path) -> Vec<String> {
    [
        "src/ui/input/keystroke.rs",
        "src/ui/shortcut_registry/model.rs",
        "src/ui/shortcut_registry/inventory.rs",
        "src/ui/shortcut_registry/dispatch.rs",
        TERMINAL_TRANSLATION,
    ]
    .into_iter()
    .filter(|relative| !root.join(relative).is_file())
    .map(|relative| format!("{relative}: required shortcut architecture owner is missing"))
    .collect()
}

pub(crate) fn check_source(path: &Path, source: &str) -> Vec<String> {
    let path_text = slash_path(path);
    if is_test_fixture(&path_text) {
        return Vec::new();
    }
    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(error) => {
            return vec![format!(
                "{}: shortcut architecture scan could not parse Rust source: {error}",
                path.display()
            )];
        }
    };
    let mut visitor = ShortcutVisitor::default();
    syn::visit::Visit::visit_file(&mut visitor, &file);
    let mut findings = Vec::new();

    if visitor.detected.contains(&Detected::CrosstermKeyTypes) && path_text != TERMINAL_TRANSLATION
    {
        findings.push(format!(
            "{}: Crossterm key interpretation is outside the terminal translation boundary",
            path.display()
        ));
    }
    let owns_logical_keys = path_text.starts_with(REGISTRY_ROOT)
        || path_text == TERMINAL_TRANSLATION
        || path_text == KEYSTROKE_MODEL
        || UI_REEXPORTS.contains(&path_text.as_str());
    if visitor.detected.contains(&Detected::LogicalKey) && !owns_logical_keys {
        findings.push(format!(
            "{}: raw LogicalKey interpretation is outside the shortcut registry dispatcher",
            path.display()
        ));
    }
    let owns_bindings = path_text.starts_with(REGISTRY_ROOT) || path_text == "src/ui/mod.rs";
    if visitor.detected.contains(&Detected::ShortcutBinding) && !owns_bindings {
        findings.push(format!(
            "{}: ShortcutBinding declarations are outside the shortcut registry",
            path.display()
        ));
    }
    if path_text == "src/ui/shortcut_metadata.rs" {
        findings.push(format!(
            "{}: parallel shortcut metadata must be projected from the registry",
            path.display()
        ));
    }
    if visitor.detected.contains(&Detected::CommandsInventory)
        && path_text != "src/ui/shortcut_registry/model.rs"
    {
        findings.push(format!(
            "{}: Commands inventory is outside the shortcut registry action owner",
            path.display()
        ));
    }
    if visitor
        .detected
        .contains(&Detected::SemanticCharacterLiteral)
        && !path_text.starts_with(REGISTRY_ROOT)
    {
        findings.push(format!(
            "{}: semantic character binding is outside the shortcut registry",
            path.display()
        ));
    }
    if visitor.detected.contains(&Detected::KeybindingsAccess)
        && !path_text.starts_with(REGISTRY_ROOT)
        && !KEYBINDING_PROJECTION_OWNERS.contains(&path_text.as_str())
    {
        findings.push(format!(
            "{}: configured keybinding access is outside registry loading or presentation",
            path.display()
        ));
    }
    findings
}

#[derive(Default)]
struct ShortcutVisitor {
    detected: BTreeSet<Detected>,
    semantic_character_bindings: Vec<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Detected {
    CrosstermKeyTypes,
    LogicalKey,
    ShortcutBinding,
    CommandsInventory,
    SemanticCharacterLiteral,
    KeybindingsAccess,
}

impl<'ast> syn::visit::Visit<'ast> for ShortcutVisitor {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !has_test_configuration(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !has_test_configuration(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !has_test_configuration(&item.attrs) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        for segment in &path.segments {
            match segment.ident.to_string().as_str() {
                "KeyCode" | "KeyModifiers" | "KeyEventKind" => {
                    self.detected.insert(Detected::CrosstermKeyTypes);
                }
                "LogicalKey" => {
                    self.detected.insert(Detected::LogicalKey);
                }
                "ShortcutBinding" => {
                    self.detected.insert(Detected::ShortcutBinding);
                }
                _ => {}
            }
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        match ident.to_string().as_str() {
            "KeyCode" | "KeyModifiers" | "KeyEventKind" => {
                self.detected.insert(Detected::CrosstermKeyTypes);
            }
            "LogicalKey" => {
                self.detected.insert(Detected::LogicalKey);
            }
            "ShortcutBinding" => {
                self.detected.insert(Detected::ShortcutBinding);
            }
            _ => {}
        }
    }

    fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
        if item.ident == "COMMANDS" {
            self.detected.insert(Detected::CommandsInventory);
        }
        syn::visit::visit_impl_item_const(self, item);
    }

    fn visit_pat_tuple_struct(&mut self, pattern: &'ast syn::PatTupleStruct) {
        let semantic_character = pattern.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Character" | "PrimaryCharacter" | "PrimaryShiftCharacter"
            )
        });
        if semantic_character
            && pattern
                .elems
                .iter()
                .any(|element| matches!(element, syn::Pat::Lit(_)))
        {
            self.detected.insert(Detected::SemanticCharacterLiteral);
        }
        syn::visit::visit_pat_tuple_struct(self, pattern);
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let mut bindings = Vec::new();
        collect_semantic_character_bindings(&arm.pat, &mut bindings);
        self.semantic_character_bindings.push(bindings);
        syn::visit::visit_pat(self, &arm.pat);
        syn::visit::visit_expr(self, &arm.body);
        self.semantic_character_bindings.pop();
    }

    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        let matched_binding = match expression.expr.as_ref() {
            syn::Expr::Path(path) => path.path.get_ident().map(ToString::to_string),
            _ => None,
        };
        if matched_binding.is_some_and(|binding| {
            self.semantic_character_bindings
                .iter()
                .flatten()
                .any(|candidate| candidate == &binding)
        }) && expression
            .arms
            .iter()
            .any(|arm| pattern_contains_character_literal(&arm.pat))
        {
            self.detected.insert(Detected::SemanticCharacterLiteral);
        }
        syn::visit::visit_expr_match(self, expression);
    }

    fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
        if matches!(&field.member, syn::Member::Named(identifier) if identifier == "keybindings") {
            self.detected.insert(Detected::KeybindingsAccess);
        }
        syn::visit::visit_expr_field(self, field);
    }
}

fn collect_semantic_character_bindings(pattern: &syn::Pat, bindings: &mut Vec<String>) {
    match pattern {
        syn::Pat::TupleStruct(tuple)
            if tuple.path.segments.last().is_some_and(|segment| {
                matches!(
                    segment.ident.to_string().as_str(),
                    "Character" | "PrimaryCharacter" | "PrimaryShiftCharacter"
                )
            }) =>
        {
            for element in &tuple.elems {
                if let syn::Pat::Ident(identifier) = element {
                    bindings.push(identifier.ident.to_string());
                }
            }
        }
        syn::Pat::Or(or) => {
            for case in &or.cases {
                collect_semantic_character_bindings(case, bindings);
            }
        }
        syn::Pat::Guard(guard) => collect_semantic_character_bindings(&guard.pat, bindings),
        syn::Pat::Paren(paren) => collect_semantic_character_bindings(&paren.pat, bindings),
        syn::Pat::Reference(reference) => {
            collect_semantic_character_bindings(&reference.pat, bindings);
        }
        _ => {}
    }
}

fn pattern_contains_character_literal(pattern: &syn::Pat) -> bool {
    match pattern {
        syn::Pat::Lit(literal) => matches!(literal.lit, syn::Lit::Char(_)),
        syn::Pat::Or(or) => or.cases.iter().any(pattern_contains_character_literal),
        syn::Pat::Guard(guard) => pattern_contains_character_literal(&guard.pat),
        syn::Pat::Paren(paren) => pattern_contains_character_literal(&paren.pat),
        syn::Pat::Reference(reference) => pattern_contains_character_literal(&reference.pat),
        _ => false,
    }
}

fn has_test_configuration(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        matches!(
            &attribute.meta,
            syn::Meta::List(list)
                if list.path.is_ident("cfg") && list.tokens.to_string().contains("test")
        )
    })
}

fn is_test_fixture(path: &str) -> bool {
    path.ends_with("/tests.rs")
        || path.contains("/tests/")
        || path.ends_with("_tests.rs")
        || path.starts_with("tests/")
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_owners_and_explicit_test_fixtures_are_accepted() {
        assert!(
            check_source(
                Path::new("src/ui/shortcut_registry/inventory.rs"),
                "use crate::ui::LogicalKey; const KEY: LogicalKey = LogicalKey::Enter;",
            )
            .is_empty()
        );
        assert!(
            check_source(
                Path::new("src/adapters/terminal/input/translation.rs"),
                "use crossterm::event::KeyCode; fn decode(code: KeyCode) {}",
            )
            .is_empty()
        );
        assert!(
            check_source(
                Path::new("src/ui/tests/shortcut_fixture.rs"),
                "use crate::ui::{LogicalKey, ShortcutBinding};",
            )
            .is_empty()
        );
    }

    #[test]
    fn raw_terminal_and_logical_key_routes_outside_their_owners_are_rejected() {
        let terminal = check_source(
            Path::new("src/ui/app.rs"),
            "use crossterm::event::{KeyCode, KeyModifiers};",
        );
        assert!(terminal[0].contains("terminal translation boundary"));

        let logical = check_source(
            Path::new("src/ui/app/help.rs"),
            "use crate::ui::LogicalKey; fn route(key: LogicalKey) {}",
        );
        assert!(logical[0].contains("registry dispatcher"));
    }

    #[test]
    fn parallel_bindings_metadata_and_commands_inventories_are_rejected() {
        let binding = check_source(
            Path::new("src/ui/settings.rs"),
            "use crate::ui::ShortcutBinding; fn bind(value: ShortcutBinding) {}",
        );
        assert!(binding[0].contains("outside the shortcut registry"));

        let metadata = check_source(Path::new("src/ui/shortcut_metadata.rs"), "fn label() {}");
        assert!(metadata[0].contains("parallel shortcut metadata"));

        let commands = check_source(
            Path::new("src/ui/app/palette/command.rs"),
            "struct Command; impl Command { const COMMANDS: [Self; 0] = []; }",
        );
        assert!(commands[0].contains("Commands inventory"));
    }

    #[test]
    fn semantic_character_routes_and_configured_dispatch_outside_registry_are_rejected() {
        let literal = check_source(
            Path::new("src/ui/app/help.rs"),
            "fn route(key: UiKey) { match key { UiKey::Character('j') => {}, _ => {} } }",
        );
        assert!(literal[0].contains("semantic character binding"));

        let nested = check_source(
            Path::new("src/ui/browser/management.rs"),
            "fn route(key: UiKey) { match key { UiKey::Character(character) => match character { 'R' => rename(), _ => {} }, _ => {} } }",
        );
        assert!(nested[0].contains("semantic character binding"));

        let configured = check_source(
            Path::new("src/ui/app/help.rs"),
            "fn route(app: &App, value: char) -> bool { value == app.settings.keybindings.help }",
        );
        assert!(configured[0].contains("configured keybinding access"));

        let parallel_help = check_source(
            Path::new("src/ui/shortcuts.rs"),
            "fn label(app: &App) -> char { app.settings.keybindings.help }",
        );
        assert!(parallel_help[0].contains("configured keybinding access"));
    }

    #[test]
    fn literal_text_insertion_and_typed_action_consumers_are_accepted() {
        let source = "fn route(key: UiKey) { match key { UiKey::Character(character) => insert(character), UiKey::Shortcut(action) => execute(action), _ => {} } }";
        assert!(check_source(Path::new("src/ui/app/query.rs"), source).is_empty());
    }
}
