use std::sync::OnceLock;

use lsp_types::{CompletionItem, CompletionItemKind, Documentation};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum CompletionScope {
    Global,
    Local,
}

#[derive(Debug, Clone)]
pub(super) struct CompletionEntry {
    label: String,
    kind: CompletionItemKind,
    detail: Option<String>,
    documentation: Option<String>,
    scope: CompletionScope,
}

impl CompletionEntry {
    pub(super) fn local(
        label: impl Into<String>,
        kind: CompletionItemKind,
        detail: Option<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind,
            detail,
            documentation: None,
            scope: CompletionScope::Local,
        }
    }
}

pub(super) fn global_completions() -> &'static [CompletionEntry] {
    static COMPLETIONS: OnceLock<Vec<CompletionEntry>> = OnceLock::new();
    COMPLETIONS.get_or_init(build_global_completions)
}

pub(super) fn items_at(
    completions: &[CompletionEntry],
    imported_packages: &[String],
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    if let Some(items) = import_string_items(source, offset) {
        return items;
    }
    if let Some(items) = package_member_items(imported_packages, source, offset) {
        return items;
    }

    let prefix = completion_prefix(source, offset);
    let mut items = completions
        .iter()
        .filter(|entry| prefix.is_empty() || entry.label.starts_with(&prefix))
        .map(|entry| (completion_sort_key(entry), completion_item(entry)))
        .collect::<Vec<_>>();
    items.sort_by(|(lhs_key, _), (rhs_key, _)| lhs_key.cmp(rhs_key));
    items.dedup_by(|(_, lhs), (_, rhs)| lhs.label == rhs.label);
    items.into_iter().map(|(_, item)| item).collect()
}

fn import_string_items(source: &str, offset: usize) -> Option<Vec<CompletionItem>> {
    let line = line_prefix(source, offset);
    let quote = line.rfind('"')?;
    let before_quote = line[..quote].trim_start();
    if !before_quote.starts_with("import") {
        return None;
    }

    let prefix = &line[quote + 1..];
    Some(
        package_entries()
            .iter()
            .filter(|entry| entry.label.starts_with(prefix))
            .map(completion_item)
            .collect(),
    )
}

fn package_member_items(
    imported_packages: &[String],
    source: &str,
    offset: usize,
) -> Option<Vec<CompletionItem>> {
    let line = line_prefix(source, offset);
    let dot = line.rfind('.')?;
    let pkg_start = line[..dot]
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| (!is_completion_char(ch)).then_some(idx + ch.len_utf8()))
        .unwrap_or(0);
    let pkg_name = &line[pkg_start..dot];
    if pkg_name.is_empty() || !imported_packages.iter().any(|pkg| pkg == pkg_name) {
        return None;
    }

    let pkg = purple_garden_std::resolve_pkg(pkg_name)?;
    let prefix = &line[dot + 1..];
    let mut items = pkg
        .overload_groups()
        .into_iter()
        .filter(|(name, _)| prefix.is_empty() || name.starts_with(prefix))
        .map(|(name, variants)| {
            completion_item(&CompletionEntry {
                label: name.to_owned(),
                kind: CompletionItemKind::FUNCTION,
                detail: Some(completion_detail_for_fns(name, &variants)),
                documentation: function_doc(&format!("{pkg_name}.{name}"), &variants),
                scope: CompletionScope::Local,
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|lhs, rhs| lhs.label.cmp(&rhs.label));
    Some(items)
}

pub(super) fn function_signature(name: &str, fun: &purple_garden_runtime::Fn<'_>) -> String {
    let args = fun
        .args
        .iter()
        .enumerate()
        .map(|(idx, ty)| {
            fun.arg_names
                .get(idx)
                .map_or_else(|| ty.to_string(), |name| format!("{name}: {ty}"))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("fn {name}({args}) {}", fun.ret)
}

fn build_global_completions() -> Vec<CompletionEntry> {
    let mut completions = Vec::new();
    for keyword in ["import", "let", "fn", "match", "as", "true", "false"] {
        let doc = crate::doc::language_doc(keyword).map(completion_doc);
        completions.push(CompletionEntry {
            label: keyword.to_owned(),
            kind: CompletionItemKind::KEYWORD,
            detail: Some("keyword".to_owned()),
            documentation: doc,
            scope: CompletionScope::Global,
        });
    }
    for ty in ["Str", "Int", "Double", "Bool", "Void", "Foreign"] {
        let doc = crate::doc::type_doc(ty).map(type_completion_doc);
        completions.push(CompletionEntry {
            label: ty.to_owned(),
            kind: CompletionItemKind::TYPE_PARAMETER,
            detail: Some("type".to_owned()),
            documentation: doc,
            scope: CompletionScope::Global,
        });
    }
    for pkg in purple_garden_std::STD {
        collect_pkg_completions(pkg, None, &mut completions);
    }
    completions
}

fn package_entries() -> &'static [CompletionEntry] {
    static PACKAGES: OnceLock<Vec<CompletionEntry>> = OnceLock::new();
    PACKAGES.get_or_init(|| {
        let mut completions = Vec::new();
        for pkg in purple_garden_std::STD {
            collect_package_entries(pkg, None, &mut completions);
        }
        completions
    })
}

fn collect_package_entries(
    pkg: &'static purple_garden_runtime::Pkg,
    parent: Option<&str>,
    completions: &mut Vec<CompletionEntry>,
) {
    let path = parent.map_or_else(
        || pkg.name.to_owned(),
        |parent| format!("{parent}/{}", pkg.name),
    );
    completions.push(CompletionEntry {
        label: path.clone(),
        kind: CompletionItemKind::MODULE,
        detail: Some(format!("import \"{}\"", path)),
        documentation: package_doc(&path, pkg),
        scope: CompletionScope::Global,
    });
    for sub in pkg.pkgs {
        collect_package_entries(sub, Some(&path), completions);
    }
}

fn collect_pkg_completions(
    pkg: &'static purple_garden_runtime::Pkg,
    parent: Option<&str>,
    completions: &mut Vec<CompletionEntry>,
) {
    let path = parent.map_or_else(
        || pkg.name.to_owned(),
        |parent| format!("{parent}/{}", pkg.name),
    );
    completions.push(CompletionEntry {
        label: path.clone(),
        kind: CompletionItemKind::MODULE,
        detail: Some(format!("import \"{}\"", path)),
        documentation: package_doc(&path, pkg),
        scope: CompletionScope::Global,
    });

    for (name, variants) in pkg.overload_groups() {
        completions.push(CompletionEntry {
            label: format!("{}.{}", path, name),
            kind: CompletionItemKind::FUNCTION,
            detail: Some(completion_detail_for_fns(name, &variants)),
            documentation: function_doc(&format!("{path}.{name}"), &variants),
            scope: CompletionScope::Global,
        });
    }

    for sub in pkg.pkgs {
        collect_pkg_completions(sub, Some(&path), completions);
    }
}

fn completion_detail_for_fns(name: &str, variants: &[&purple_garden_runtime::Fn<'_>]) -> String {
    if let [single] = variants {
        return function_signature(name, single);
    }
    format!("fn {name} ({} overloads)", variants.len())
}

fn completion_doc(doc: &purple_garden_frontend::lex::KeywordDoc) -> String {
    format!(
        "{} {}\n\n{}\n\n{}",
        doc.kind,
        doc.name,
        doc.doc,
        crate::doc::command(doc.name)
    )
}

fn type_completion_doc(doc: &purple_garden_frontend::lex::TypeDoc) -> String {
    format!(
        "type {}\n\n{}\n\n{}",
        doc.name,
        doc.doc,
        crate::doc::command(doc.name)
    )
}

fn package_doc(path: &str, pkg: &purple_garden_runtime::Pkg) -> Option<String> {
    let command = crate::doc::command(path);
    if pkg.doc.is_empty() {
        Some(command)
    } else {
        Some(format!("{}\n\n{command}", pkg.doc))
    }
}

fn function_doc(name: &str, variants: &[&purple_garden_runtime::Fn<'_>]) -> Option<String> {
    let command = crate::doc::command(name);
    let display_name = name.rsplit_once('.').map_or(name, |(_, name)| name);
    let doc = crate::doc::render_function(display_name, variants);
    Some(format!("{}\n{command}", doc.trim_end()))
}

fn completion_prefix(source: &str, offset: usize) -> String {
    let clamped = offset.min(source.len());
    let start = source[..clamped]
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| (!is_completion_char(ch)).then_some(idx + ch.len_utf8()))
        .unwrap_or(0);
    source[start..clamped].to_owned()
}

fn line_prefix(source: &str, offset: usize) -> &str {
    let clamped = offset.min(source.len());
    let start = source[..clamped]
        .as_bytes()
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |idx| idx + 1);
    &source[start..clamped]
}

fn is_completion_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '/' | '.')
}

fn completion_item(entry: &CompletionEntry) -> CompletionItem {
    CompletionItem {
        label: entry.label.clone(),
        kind: Some(entry.kind),
        detail: entry.detail.clone(),
        documentation: entry
            .documentation
            .as_ref()
            .map(|doc| Documentation::String(doc.clone())),
        sort_text: Some(completion_sort_key(entry)),
        ..Default::default()
    }
}

fn completion_sort_key(entry: &CompletionEntry) -> String {
    match entry.scope {
        CompletionScope::Local => format!("0_{}", entry.label),
        CompletionScope::Global => format!("1_{}", entry.label),
    }
}
