use std::collections::BTreeSet;
use std::path::PathBuf;

use dioxus::prelude::*;

use savhub_local::config::{add_project, read_projects_list, remove_project};
use savhub_local::presets::{
    EnableProjectRepoSkillResult, ProjectSkillConflict, ProjectSkillConflictChoice,
    ResolvedSkillSources, add_skills_to_preset, create_preset, delete_preset,
    disable_project_preset, disable_project_skill, enable_project_preset,
    enable_repo_skill_in_project, list_repo_skills, read_presets_store, read_project_presets,
    read_project_selector_matches, remove_skills_from_preset, resolve_project_skills_with_sources,
};

use crate::components::pagination::{self, PaginationControls};
use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const PROJECTS_PAGE_SIZE: usize = 10;
const PROJECT_SELECTORS_PAGE_SIZE: usize = 8;
const PROJECT_ENABLED_PRESETS_PAGE_SIZE: usize = 10;
const PROJECT_AVAILABLE_PRESETS_PAGE_SIZE: usize = 12;
const PROJECT_SKILLS_PAGE_SIZE: usize = 8;
const LOCAL_SKILLS_PAGE_SIZE: usize = 10;
const PRESET_CARDS_PAGE_SIZE: usize = 8;
const PRESET_SKILLS_PAGE_SIZE: usize = 12;

#[component]
pub fn ProjectsPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut version = use_signal(|| 0u32);
    let mut selected_project = use_signal(|| Option::<String>::None);
    let mut add_path_input = use_signal(String::new);
    let mut projects_page = use_signal(|| 0usize);

    let _ = *version.read();

    let projects = read_projects_list().unwrap_or_default();
    let projects_items = projects.projects.clone();
    let current_projects_page = pagination::clamp_page(
        *projects_page.read(),
        projects_items.len(),
        PROJECTS_PAGE_SIZE,
    );
    let visible_projects =
        pagination::slice_for_page(&projects_items, current_projects_page, PROJECTS_PAGE_SIZE);
    let projects_total_pages = pagination::total_pages(projects_items.len(), PROJECTS_PAGE_SIZE);
    let title = t.projects_title;

    let do_add_project = move |_| {
        let path = add_path_input.read().trim().to_string();
        if path.is_empty() {
            return;
        }
        let _ = add_project(&path);
        selected_project.set(Some(path));
        add_path_input.set(String::new());
        version.with_mut(|v| *v += 1);
    };

    let sel = selected_project.read().clone();

    rsx! {
        div { style: "display: flex; height: 100%;",
            div { style: "width: 280px; background: rgba(238, 246, 232, 0.5); border-right: 1px solid {Theme::LINE}; display: flex; flex-direction: column; overflow: hidden;",
                div { style: "padding: 20px 16px 12px;",
                    h2 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 12px;",
                        "{title}"
                    }
                    div { style: "display: flex; gap: 6px;",
                        input {
                            style: "flex: 1; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 12px; background: {Theme::BG_ELEVATED}; color: {Theme::TEXT}; outline: none;",
                            placeholder: "{t.projects_path_placeholder}",
                            value: "{add_path_input}",
                            oninput: move |e| add_path_input.set(e.value()),
                            onkeypress: move |e| {
                                if e.key() == Key::Enter {
                                    let path = add_path_input.read().trim().to_string();
                                    if !path.is_empty() {
                                        let _ = add_project(&path);
                                        selected_project.set(Some(path));
                                        add_path_input.set(String::new());
                                        version.with_mut(|v| *v += 1);
                                    }
                                }
                            },
                        }
                        button {
                            style: "padding: 6px 10px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer; white-space: nowrap;",
                            onclick: do_add_project,
                            "+"
                        }
                    }
                }

                div { style: "flex: 1; overflow-y: auto; padding: 0 8px 8px;",
                    if projects_items.is_empty() {
                        p { style: "padding: 16px 8px; font-size: 12px; color: {Theme::MUTED}; text-align: center;",
                            "{t.projects_no_projects}"
                        }
                    } else {
                        for project in visible_projects.iter() {
                            {
                                let path_str = project.path.clone();
                                let path_for_select = project.path.clone();
                                let path_for_remove = project.path.clone();
                                let is_selected = sel.as_deref() == Some(project.path.as_str());
                                let dir_name = PathBuf::from(&project.path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| project.path.clone());
                                let bg = if is_selected { Theme::ACCENT_LIGHT } else { "transparent" };
                                let text_color = if is_selected { Theme::ACCENT_STRONG } else { Theme::TEXT };

                                rsx! {
                                    div {
                                        style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; margin-bottom: 2px; background: {bg}; border-radius: 6px; cursor: pointer; transition: background 0.15s;",
                                        onclick: move |_| selected_project.set(Some(path_for_select.clone())),
                                        div { style: "flex: 1; min-width: 0;",
                                            p { style: "font-size: 13px; font-weight: 600; color: {text_color}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                "{dir_name}"
                                            }
                                            p { style: "font-size: 10px; color: {Theme::MUTED}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                "{path_str}"
                                            }
                                        }
                                        button {
                                            style: "background: none; border: none; color: {Theme::MUTED}; font-size: 14px; cursor: pointer; padding: 2px 4px; line-height: 1; flex-shrink: 0;",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                let _ = remove_project(&path_for_remove);
                                                if selected_project.read().as_deref() == Some(path_for_remove.as_str()) {
                                                    selected_project.set(None);
                                                }
                                                version.with_mut(|v| *v += 1);
                                            },
                                            "\u{00D7}"
                                        }
                                    }
                                }
                            }
                        }
                        PaginationControls {
                            current_page: current_projects_page,
                            total_pages: Some(projects_total_pages),
                            has_prev: current_projects_page > 0,
                            has_next: current_projects_page + 1 < projects_total_pages,
                            on_prev: move |_| projects_page.set(current_projects_page.saturating_sub(1)),
                            on_next: move |_| projects_page.set(current_projects_page + 1),
                        }
                    }
                }
            }

            div { style: "flex: 1; overflow-y: auto; padding: 24px;",
                if let Some(ref project_path) = sel {
                    ProjectDetail {
                        project_path: project_path.clone(),
                        version: version,
                    }
                } else {
                    div { style: "display: flex; align-items: center; justify-content: center; height: 200px;",
                        p { style: "font-size: 14px; color: {Theme::MUTED};",
                            "{t.project_select_hint}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn PresetsPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut version = use_signal(|| 0u32);
    let mut show_create = use_signal(|| false);
    let mut view_mode = use_signal(|| ViewMode::Cards);

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; white-space: nowrap; margin-right: auto;",
                    "{t.presets_title}"
                }
                // Refresh
                button {
                    title: "Refresh",
                    style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                    onclick: move |_| version += 1,
                    "\u{21BB}"
                }
                // View toggle
                ViewToggleButton {
                    mode: *view_mode.read(),
                    on_toggle: move |_| {
                        let cur = *view_mode.read();
                        view_mode.set(if cur == ViewMode::Cards { ViewMode::List } else { ViewMode::Cards });
                    },
                }
                // Create
                button {
                    style: "padding: 7px 16px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; font-size: 13px; font-weight: 700; cursor: pointer; white-space: nowrap;",
                    onclick: move |_| show_create.set(true),
                    "+ Create"
                }
            }
            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 20px 32px 32px;",
                PresetsSection { version: version, show_title: false, card_mode: *view_mode.read() == ViewMode::Cards }
            }
        }
        if *show_create.read() {
            CreatePresetModal { show: show_create, version: version }
        }
    }
}

#[component]
fn ProjectDetail(project_path: String, mut version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut show_add_skill_dialog = use_signal(|| false);
    let mut show_rescan_modal = use_signal(|| false);
    let mut selectors_page = use_signal(|| 0usize);
    let mut enabled_presets_page = use_signal(|| 0usize);
    let mut available_presets_page = use_signal(|| 0usize);
    let mut effective_skills_page = use_signal(|| 0usize);

    let _ = *version.read();

    let workdir = PathBuf::from(&project_path);
    let selector_matches = read_project_selector_matches(&workdir).unwrap_or_default();
    let enabled_presets = read_project_presets(&workdir).unwrap_or_default();
    let store = read_presets_store().unwrap_or_default();
    let effective_skills = resolve_project_skills_with_sources(&workdir, None).unwrap_or_default();
    let repo_skills = collect_repo_skill_options();
    let enabled_skill_slugs = effective_skills
        .iter()
        .map(|skill| skill.slug.clone())
        .collect::<BTreeSet<_>>();
    let selector_items = selector_matches.clone();
    let selector_current_page = pagination::clamp_page(
        *selectors_page.read(),
        selector_items.len(),
        PROJECT_SELECTORS_PAGE_SIZE,
    );
    let visible_selector_matches = pagination::slice_for_page(
        &selector_items,
        selector_current_page,
        PROJECT_SELECTORS_PAGE_SIZE,
    );
    let selector_total_pages =
        pagination::total_pages(selector_items.len(), PROJECT_SELECTORS_PAGE_SIZE);
    let enabled_preset_items = enabled_presets.clone();
    let enabled_presets_current_page = pagination::clamp_page(
        *enabled_presets_page.read(),
        enabled_preset_items.len(),
        PROJECT_ENABLED_PRESETS_PAGE_SIZE,
    );
    let visible_enabled_presets = pagination::slice_for_page(
        &enabled_preset_items,
        enabled_presets_current_page,
        PROJECT_ENABLED_PRESETS_PAGE_SIZE,
    );
    let enabled_presets_total_pages = pagination::total_pages(
        enabled_preset_items.len(),
        PROJECT_ENABLED_PRESETS_PAGE_SIZE,
    );
    let available_preset_items = store
        .presets
        .keys()
        .filter(|preset_name| !enabled_presets.contains(*preset_name))
        .cloned()
        .collect::<Vec<_>>();
    let available_presets_current_page = pagination::clamp_page(
        *available_presets_page.read(),
        available_preset_items.len(),
        PROJECT_AVAILABLE_PRESETS_PAGE_SIZE,
    );
    let visible_available_presets = pagination::slice_for_page(
        &available_preset_items,
        available_presets_current_page,
        PROJECT_AVAILABLE_PRESETS_PAGE_SIZE,
    );
    let available_presets_total_pages = pagination::total_pages(
        available_preset_items.len(),
        PROJECT_AVAILABLE_PRESETS_PAGE_SIZE,
    );
    let effective_skills_current_page = pagination::clamp_page(
        *effective_skills_page.read(),
        effective_skills.len(),
        PROJECT_SKILLS_PAGE_SIZE,
    );
    let visible_effective_skills = pagination::slice_for_page(
        &effective_skills,
        effective_skills_current_page,
        PROJECT_SKILLS_PAGE_SIZE,
    );
    let effective_skills_total_pages =
        pagination::total_pages(effective_skills.len(), PROJECT_SKILLS_PAGE_SIZE);

    let dir_name = workdir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| project_path.clone());

    rsx! {
        div { style: "display: flex; align-items: flex-start; justify-content: space-between; margin-bottom: 20px;",
            div {
                h2 { style: "font-size: 20px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 4px;",
                    "{dir_name}"
                }
                p { style: "font-size: 12px; color: {Theme::MUTED};", "{project_path}" }
            }
            button {
                style: "padding: 7px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 8px; font-size: 13px; font-weight: 600; cursor: pointer; white-space: nowrap; flex-shrink: 0;",
                onclick: move |_| show_rescan_modal.set(true),
                "{t.project_rescan}"
            }
        }

        if *show_rescan_modal.read() {
            RescanModal { project_path: project_path.clone(), show: show_rescan_modal, version: version }
        }

        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
            h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 10px;",
                "{t.project_matched_selectors} ({selector_matches.len()})"
            }

            if selector_matches.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED};", "{t.project_no_selectors}" }
            } else {
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    for matched in visible_selector_matches.iter() {
                        SelectorMatchRow {
                            key: "{matched.selector}",
                            selector: matched.selector.clone(),
                            presets: matched.presets.clone(),
                            presets_label: t.project_reason_presets,
                        }
                    }
                }
                PaginationControls {
                    current_page: selector_current_page,
                    total_pages: Some(selector_total_pages),
                    has_prev: selector_current_page > 0,
                    has_next: selector_current_page + 1 < selector_total_pages,
                    on_prev: move |_| selectors_page.set(selector_current_page.saturating_sub(1)),
                    on_next: move |_| selectors_page.set(selector_current_page + 1),
                }
            }
        }

        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
            h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 10px;",
                "{t.project_current_presets} ({enabled_presets.len()})"
            }

            if enabled_presets.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 8px;", "{t.project_no_profile}" }
            } else {
                div { style: "display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 10px;",
                    for preset_name in visible_enabled_presets.iter() {
                        {
                            let preset_for_disable = preset_name.clone();
                            let pp = project_path.clone();
                            rsx! {
                                div { style: "display: flex; align-items: center; gap: 6px; padding: 6px 10px; background: {Theme::ACCENT_LIGHT}; border-radius: 14px;",
                                    span { style: "font-size: 12px; color: {Theme::ACCENT_STRONG}; font-weight: 600;", "{preset_name}" }
                                    button {
                                        style: "background: none; border: none; color: {Theme::MUTED}; font-size: 14px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                        onclick: move |_| {
                                            let wd = PathBuf::from(&pp);
                                            let _ = disable_project_preset(&wd, &preset_for_disable);
                                            version.with_mut(|v| *v += 1);
                                        },
                                        "\u{00D7}"
                                    }
                                }
                            }
                        }
                    }
                }
                PaginationControls {
                    current_page: enabled_presets_current_page,
                    total_pages: Some(enabled_presets_total_pages),
                    has_prev: enabled_presets_current_page > 0,
                    has_next: enabled_presets_current_page + 1 < enabled_presets_total_pages,
                    on_prev: move |_| enabled_presets_page.set(enabled_presets_current_page.saturating_sub(1)),
                    on_next: move |_| enabled_presets_page.set(enabled_presets_current_page + 1),
                }
            }

            if !available_preset_items.is_empty() {
                div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px;",
                    for preset_name in visible_available_presets.iter() {
                        {
                            let name_for_enable = preset_name.clone();
                            let pp = project_path.clone();
                            rsx! {
                                button {
                                    style: "padding: 4px 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; font-size: 12px; font-weight: 500; cursor: pointer;",
                                    onclick: move |_| {
                                        let wd = PathBuf::from(&pp);
                                        let _ = enable_project_preset(&wd, &name_for_enable);
                                        version.with_mut(|v| *v += 1);
                                    },
                                    "{t.profile_bind} {name_for_enable}"
                                }
                            }
                        }
                    }
                }
                PaginationControls {
                    current_page: available_presets_current_page,
                    total_pages: Some(available_presets_total_pages),
                    has_prev: available_presets_current_page > 0,
                    has_next: available_presets_current_page + 1 < available_presets_total_pages,
                    on_prev: move |_| available_presets_page.set(available_presets_current_page.saturating_sub(1)),
                    on_next: move |_| available_presets_page.set(available_presets_current_page + 1),
                }
            }
        }

        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
            div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px;",
                h3 { style: "font-size: 13px; color: {Theme::MUTED};",
                    "{t.project_installed_skills} ({effective_skills.len()})"
                }
                button {
                    style: "padding: 6px 12px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer; white-space: nowrap;",
                    onclick: move |_| show_add_skill_dialog.set(true),
                    "{t.project_inject_skill}"
                }
            }

            if effective_skills.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED};", "{t.project_no_enabled_skills}" }
            } else {
                div { style: "border: 1px solid {Theme::LINE}; border-radius: 8px; overflow: hidden;",
                    div { style: "display: grid; grid-template-columns: minmax(180px, 1.1fr) minmax(280px, 2fr) 96px; gap: 12px; padding: 10px 14px; background: rgba(238, 246, 232, 0.62); border-bottom: 1px solid {Theme::LINE};",
                        p { style: "font-size: 12px; font-weight: 700; color: {Theme::TEXT};", "{t.project_skill_name}" }
                        p { style: "font-size: 12px; font-weight: 700; color: {Theme::TEXT};", "{t.project_skill_reason}" }
                        p { style: "font-size: 12px; font-weight: 700; color: {Theme::TEXT}; text-align: right;", "{t.project_skill_action}" }
                    }
                    for skill in visible_effective_skills.iter() {
                        {
                            let slug_for_remove = skill.slug.clone();
                            let pp = project_path.clone();
                            let reason = build_skill_reason_text(t, &skill.sources);
                            let has_manual_source = skill.sources.manual;
                            rsx! {
                                div { style: "display: grid; grid-template-columns: minmax(180px, 1.1fr) minmax(280px, 2fr) 96px; gap: 12px; padding: 12px 14px; align-items: center; border-bottom: 1px solid {Theme::LINE};",
                                    div { style: "min-width: 0;",
                                        p { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                            "{skill.display_name}"
                                        }
                                        p { style: "font-size: 11px; color: {Theme::MUTED}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                            "{skill.slug}"
                                        }
                                    }
                                    p { style: "font-size: 12px; color: {Theme::TEXT}; line-height: 1.6;",
                                        "{reason}"
                                    }
                                    div { style: "display: flex; justify-content: flex-end;",
                                        if has_manual_source {
                                            button {
                                                style: "padding: 5px 10px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.18); border-radius: 6px; font-size: 11px; font-weight: 600; cursor: pointer; white-space: nowrap;",
                                                onclick: move |_| {
                                                    let wd = PathBuf::from(&pp);
                                                    let _ = disable_project_skill(&wd, &slug_for_remove);
                                                    version.with_mut(|v| *v += 1);
                                                },
                                                "{t.projects_remove}"
                                            }
                                        } else {
                                            span { style: "font-size: 12px; color: {Theme::MUTED};", "-" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                PaginationControls {
                    current_page: effective_skills_current_page,
                    total_pages: Some(effective_skills_total_pages),
                    has_prev: effective_skills_current_page > 0,
                    has_next: effective_skills_current_page + 1 < effective_skills_total_pages,
                    on_prev: move |_| effective_skills_page.set(effective_skills_current_page.saturating_sub(1)),
                    on_next: move |_| effective_skills_page.set(effective_skills_current_page + 1),
                }
            }
        }

        if *show_add_skill_dialog.read() {
            AddProjectSkillDialog {
                project_path: project_path.clone(),
                version: version,
                skills: repo_skills,
                enabled_skill_slugs: enabled_skill_slugs.into_iter().collect(),
                add_label: t.project_inject_skill,
                enabled_label: t.profile_bound,
                empty_label: t.project_local_skills_empty,
                title: t.project_local_skills_title,
                close_label: t.close,
                conflict_label: t.project_conflict_detected,
                use_repo_label: t.project_use_repo_skill,
                keep_existing_label: t.project_keep_existing_skill,
                on_close: move |_| show_add_skill_dialog.set(false),
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RepoSkillOption {
    repo_name: String,
    slug: String,
    display_name: String,
    location: String,
}

fn collect_repo_skill_options() -> Vec<RepoSkillOption> {
    let mut options = list_repo_skills()
        .unwrap_or_default()
        .into_iter()
        .map(|skill| RepoSkillOption {
            repo_name: skill.repo_name,
            slug: skill.skill.slug,
            display_name: skill.skill.display_name,
            location: skill.skill.folder.display().to_string(),
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.slug.cmp(&right.slug))
            .then_with(|| left.repo_name.cmp(&right.repo_name))
    });
    options
}

fn build_skill_reason_text(t: &i18n::Texts, sources: &ResolvedSkillSources) -> String {
    let mut parts = Vec::new();
    if !sources.presets.is_empty() {
        parts.push(format!(
            "{}: {}",
            t.project_reason_presets,
            sources.presets.join(", ")
        ));
    }
    if !sources.selectors.is_empty() {
        parts.push(format!(
            "{}: {}",
            t.project_reason_selectors,
            sources.selectors.join(", ")
        ));
    }
    if !sources.flocks.is_empty() {
        parts.push(format!(
            "{}: {}",
            t.project_reason_flocks,
            sources.flocks.join(", ")
        ));
    }
    if sources.manual {
        parts.push(t.project_reason_manual.to_string());
    }

    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join("; ")
    }
}

#[component]
fn SelectorMatchRow(
    selector: String,
    presets: Vec<String>,
    presets_label: &'static str,
) -> Element {
    let presets_text = if presets.is_empty() {
        "-".to_string()
    } else {
        presets.join(", ")
    };

    rsx! {
        div { style: "padding: 12px 14px; background: {Theme::BG_ELEVATED}; border: 1px solid {Theme::LINE}; border-radius: 8px;",
            p { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 4px;",
                "{selector}"
            }
            p { style: "font-size: 12px; color: {Theme::MUTED}; line-height: 1.5;",
                "{presets_label}: {presets_text}"
            }
        }
    }
}

#[component]
fn AddProjectSkillDialog(
    project_path: String,
    mut version: Signal<u32>,
    skills: Vec<RepoSkillOption>,
    enabled_skill_slugs: Vec<String>,
    title: &'static str,
    add_label: &'static str,
    enabled_label: &'static str,
    empty_label: &'static str,
    close_label: &'static str,
    conflict_label: &'static str,
    use_repo_label: &'static str,
    keep_existing_label: &'static str,
    on_close: EventHandler<()>,
) -> Element {
    let enabled = enabled_skill_slugs.into_iter().collect::<BTreeSet<_>>();
    let mut skills_page = use_signal(|| 0usize);
    let mut pending_conflict = use_signal(|| Option::<ProjectSkillConflict>::None);
    let mut status_msg = use_signal(|| Option::<String>::None);
    let mut backdrop_pressed = use_signal(|| false);
    let current_page =
        pagination::clamp_page(*skills_page.read(), skills.len(), LOCAL_SKILLS_PAGE_SIZE);
    let visible_skills = pagination::slice_for_page(&skills, current_page, LOCAL_SKILLS_PAGE_SIZE);
    let total_pages = pagination::total_pages(skills.len(), LOCAL_SKILLS_PAGE_SIZE);

    rsx! {
        div {
            style: "position: fixed; inset: 0; background: rgba(26, 46, 24, 0.38); display: flex; align-items: center; justify-content: center; padding: 24px; z-index: 1000;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { on_close.call(()); } backdrop_pressed.set(false); },
            div {
                style: "width: 100%; max-width: 760px; background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 12px; box-shadow: 0 24px 64px rgba(26, 46, 24, 0.18); padding: 20px;",
                onmousedown: move |evt| evt.stop_propagation(),
                onmouseup: move |evt| evt.stop_propagation(),
                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 14px;",
                    h2 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                        "{title}"
                    }
                    button {
                        style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| on_close.call(()),
                        "\u{00D7}"
                    }
                }

                if let Some(conflict) = pending_conflict.read().as_ref() {
                    div { style: "margin-bottom: 14px; padding: 12px 14px; background: rgba(191, 126, 26, 0.08); border: 1px solid rgba(191, 126, 26, 0.22); border-radius: 8px;",
                        p { style: "font-size: 12px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 8px;",
                            "{conflict_label}"
                        }
                        p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 4px; font-family: Consolas, 'SFMono-Regular', monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                            "{conflict.repo_skill_path.display()}"
                        }
                        p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 10px; font-family: Consolas, 'SFMono-Regular', monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                            "{conflict.existing_skill_path.display()}"
                        }
                        div { style: "display: flex; gap: 8px; flex-wrap: wrap;",
                            button {
                                style: "padding: 6px 12px; background: linear-gradient(135deg, #6aa84f 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer;",
                                onclick: {
                                    let project_path = project_path.clone();
                                    let conflict = conflict.clone();
                                    move |_| {
                                        let wd = PathBuf::from(&project_path);
                                        let sources = ResolvedSkillSources {
                                            manual: true,
                                            ..ResolvedSkillSources::default()
                                        };
                                        match enable_repo_skill_in_project(
                                            &wd,
                                            &conflict.repo_name,
                                            &conflict.slug,
                                            ProjectSkillConflictChoice::UseRepo,
                                            sources,
                                        ) {
                                            Ok(EnableProjectRepoSkillResult::Enabled { .. }) => {
                                                pending_conflict.set(None);
                                                status_msg.set(None);
                                                version.with_mut(|value| *value += 1);
                                                on_close.call(());
                                            }
                                            Ok(EnableProjectRepoSkillResult::KeptExisting { .. }) => {
                                                pending_conflict.set(None);
                                                on_close.call(());
                                            }
                                            Ok(EnableProjectRepoSkillResult::Conflict(_)) => {
                                                status_msg.set(Some("unexpected conflict state".to_string()));
                                            }
                                            Err(error) => status_msg.set(Some(error.to_string())),
                                        }
                                    }
                                },
                                "{use_repo_label}"
                            }
                            button {
                                style: "padding: 6px 12px; background: {Theme::BG_ELEVATED}; color: {Theme::TEXT}; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer;",
                                onclick: {
                                    let project_path = project_path.clone();
                                    let conflict = conflict.clone();
                                    move |_| {
                                        let wd = PathBuf::from(&project_path);
                                        let sources = ResolvedSkillSources {
                                            manual: true,
                                            ..ResolvedSkillSources::default()
                                        };
                                        match enable_repo_skill_in_project(
                                            &wd,
                                            &conflict.repo_name,
                                            &conflict.slug,
                                            ProjectSkillConflictChoice::KeepExisting,
                                            sources,
                                        ) {
                                            Ok(EnableProjectRepoSkillResult::Enabled { .. })
                                            | Ok(EnableProjectRepoSkillResult::KeptExisting { .. }) => {
                                                pending_conflict.set(None);
                                                status_msg.set(None);
                                                on_close.call(());
                                            }
                                            Ok(EnableProjectRepoSkillResult::Conflict(_)) => {
                                                status_msg.set(Some("unexpected conflict state".to_string()));
                                            }
                                            Err(error) => status_msg.set(Some(error.to_string())),
                                        }
                                    }
                                },
                                "{keep_existing_label}"
                            }
                        }
                    }
                }

                if let Some(message) = status_msg.read().as_ref() {
                    p { style: "font-size: 12px; color: {Theme::DANGER}; margin-bottom: 12px;",
                        "{message}"
                    }
                }

                if skills.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED};",
                        "{empty_label}"
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 10px; max-height: 520px; overflow-y: auto;",
                        for skill in visible_skills.iter() {
                            {
                                let is_enabled = enabled.contains(&skill.slug);
                                rsx! {
                                    div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 14px; background: {Theme::BG_ELEVATED}; border: 1px solid {Theme::LINE}; border-radius: 8px;",
                                        div { style: "min-width: 0; flex: 1;",
                                            p { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                "{skill.display_name}"
                                            }
                                            p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 2px;",
                                                "{skill.repo_name} / {skill.slug}"
                                            }
                                            p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 4px; font-family: Consolas, 'SFMono-Regular', monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                "{skill.location}"
                                            }
                                        }
                                        div { style: "display: flex; align-items: center; gap: 8px; flex-shrink: 0;",
                                            if is_enabled {
                                                span { style: "font-size: 11px; font-weight: 600; color: {Theme::ACCENT_STRONG};",
                                                    "{enabled_label}"
                                                }
                                            }
                                            button {
                                                style: "padding: 6px 12px; background: linear-gradient(135deg, #6aa84f 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer; white-space: nowrap;",
                                                onclick: {
                                                    let project_path = project_path.clone();
                                                    let skill = skill.clone();
                                                    move |_| {
                                                        let wd = PathBuf::from(&project_path);
                                                        let sources = ResolvedSkillSources {
                                                            manual: true,
                                                            ..ResolvedSkillSources::default()
                                                        };
                                                        match enable_repo_skill_in_project(
                                                            &wd,
                                                            &skill.repo_name,
                                                            &skill.slug,
                                                            ProjectSkillConflictChoice::Ask,
                                                            sources,
                                                        ) {
                                                            Ok(EnableProjectRepoSkillResult::Enabled { .. }) => {
                                                                pending_conflict.set(None);
                                                                status_msg.set(None);
                                                                version.with_mut(|value| *value += 1);
                                                                on_close.call(());
                                                            }
                                                            Ok(EnableProjectRepoSkillResult::KeptExisting { .. }) => {
                                                                pending_conflict.set(None);
                                                                status_msg.set(None);
                                                                on_close.call(());
                                                            }
                                                            Ok(EnableProjectRepoSkillResult::Conflict(conflict)) => {
                                                                pending_conflict.set(Some(conflict));
                                                                status_msg.set(None);
                                                            }
                                                            Err(error) => status_msg.set(Some(error.to_string())),
                                                        }
                                                    }
                                                },
                                                "{add_label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    PaginationControls {
                        current_page: current_page,
                        total_pages: Some(total_pages),
                        has_prev: current_page > 0,
                        has_next: current_page + 1 < total_pages,
                        on_prev: move |_| skills_page.set(current_page.saturating_sub(1)),
                        on_next: move |_| skills_page.set(current_page + 1),
                    }
                }
            }
        }
    }
}

#[component]
fn PresetsSection(
    mut version: Signal<u32>,
    show_title: bool,
    #[props(default = true)] card_mode: bool,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut presets_page = use_signal(|| 0usize);

    let _ = *version.read();

    let store = read_presets_store().unwrap_or_default();
    let preset_items = store
        .presets
        .iter()
        .map(|(name, profile)| (name.clone(), profile.clone()))
        .collect::<Vec<_>>();
    let current_page = pagination::clamp_page(
        *presets_page.read(),
        preset_items.len(),
        PRESET_CARDS_PAGE_SIZE,
    );
    let visible_presets =
        pagination::slice_for_page(&preset_items, current_page, PRESET_CARDS_PAGE_SIZE);
    let total_pages = pagination::total_pages(preset_items.len(), PRESET_CARDS_PAGE_SIZE);

    rsx! {
        if show_title {
            h2 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 16px;",
                "{t.presets_title}"
            }
        }

        if store.presets.is_empty() {
            div { style: "text-align: center; padding: 24px; color: {Theme::MUTED}; font-size: 14px;",
                "{t.profile_no_profiles}"
            }
        } else {
            { let container_style = if card_mode {
                "display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px;"
            } else {
                "display: flex; flex-direction: column; gap: 10px;"
            };
            rsx! {
            div { style: "{container_style}",
                for (name, profile) in visible_presets.iter() {
                    PresetCard {
                        key: "{name}",
                        name: name.clone(),
                        description: profile.description.clone(),
                        skills: profile.skills.clone(),
                        version: version,
                    }
                }
            }
            PaginationControls {
                current_page: current_page,
                total_pages: Some(total_pages),
                has_prev: current_page > 0,
                has_next: current_page + 1 < total_pages,
                on_prev: move |_| presets_page.set(current_page.saturating_sub(1)),
                on_next: move |_| presets_page.set(current_page + 1),
            }
            } // rsx
            } // closure
        }

    }
}

/// Modal for creating a new preset with multi-select from installed skills.
#[component]
fn CreatePresetModal(show: Signal<bool>, version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut backdrop_pressed = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut desc = use_signal(String::new);
    let mut selected = use_signal(|| BTreeSet::<String>::new());
    let mut search = use_signal(String::new);
    let mut selected_flocks = use_signal(|| BTreeSet::<String>::new());
    let mut flock_search = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);

    let installed = savhub_local::registry::list_installed_slugs().unwrap_or_default();
    let all_flock_slugs = savhub_local::registry::list_flock_slugs().unwrap_or_default();
    let search_val = search.read().to_lowercase();
    let suggestions: Vec<&String> = if search_val.is_empty() {
        Vec::new()
    } else {
        installed
            .iter()
            .filter(|s| !selected.read().contains(*s) && s.to_lowercase().contains(&search_val))
            .take(20)
            .collect()
    };
    let flock_search_val = flock_search.read().to_lowercase();
    let flock_suggestions: Vec<&String> = if flock_search_val.is_empty() {
        Vec::new()
    } else {
        all_flock_slugs
            .iter()
            .filter(|s| {
                !selected_flocks.read().contains(*s) && s.to_lowercase().contains(&flock_search_val)
            })
            .take(20)
            .collect()
    };

    let do_save = move |_| {
        let n = name.read().trim().to_string();
        if n.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }
        let desc_val = desc.read().trim().to_string();
        let d = if desc_val.is_empty() {
            None
        } else {
            Some(desc_val.as_str())
        };
        match create_preset(&n, d) {
            Ok(()) => {
                let slugs: Vec<String> = selected.read().iter().cloned().collect();
                if !slugs.is_empty() {
                    let _ = add_skills_to_preset(&n, &slugs);
                }
                let flock_slugs: Vec<String> = selected_flocks.read().iter().cloned().collect();
                if !flock_slugs.is_empty() {
                    let _ = savhub_local::presets::add_flocks_to_preset(&n, &flock_slugs);
                }
                show.set(false);
                version.with_mut(|v| *v += 1);
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };

    let label_style = format!(
        "font-size: 12px; font-weight: 700; color: {}; margin-bottom: 4px;",
        Theme::MUTED
    );
    let input_style = format!(
        "width: 100%; padding: 8px 12px; border: 1px solid {}; border-radius: 8px; font-size: 13px; background: white; color: {};",
        Theme::LINE,
        Theme::TEXT
    );

    rsx! {
        div {
            style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 1000; display: flex; align-items: center; justify-content: center; padding: 24px;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { show.set(false); } backdrop_pressed.set(false); },
            div {
                style: "background: {Theme::PANEL}; border: 2px solid {Theme::ACCENT}; border-radius: 18px; padding: 28px; width: 600px; max-width: 92vw; max-height: 92vh; overflow-y: auto; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.25);",
                onmousedown: move |evt: Event<MouseData>| evt.stop_propagation(),
                onmouseup: move |evt: Event<MouseData>| evt.stop_propagation(),
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px;",
                    h2 { style: "font-size: 18px; font-weight: 800; color: {Theme::TEXT};", "{t.profile_create}" }
                    button { style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| show.set(false), "\u{00D7}"
                    }
                }
                div { style: "display: flex; flex-direction: column; gap: 14px;",
                    div {
                        p { style: "{label_style}", "{t.profile_name}" }
                        input { r#type: "text", value: "{name}", placeholder: "{t.profile_create_placeholder}", style: "{input_style}",
                            oninput: move |e: Event<FormData>| name.set(e.value().to_string()),
                        }
                    }
                    div {
                        p { style: "{label_style}", "{t.profile_description}" }
                        input { r#type: "text", value: "{desc}", style: "{input_style}",
                            oninput: move |e: Event<FormData>| desc.set(e.value().to_string()),
                        }
                    }
                    // Skills — search-to-add
                    div {
                        p { style: "{label_style}", "{t.profile_skills}" }
                        if !selected.read().is_empty() {
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px;",
                                for skill in selected.read().iter() {
                                    { let slug = skill.clone();
                                      rsx! {
                                        span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                            "{slug}"
                                            button {
                                                style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                                onclick: { let slug = slug.clone(); move |_| { let s = slug.clone(); selected.with_mut(|set| { set.remove(&s); }); } },
                                                "\u{00D7}"
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        input { r#type: "text", value: "{search}", placeholder: "Search installed skills...",
                            style: "width: 100%; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                            oninput: move |e: Event<FormData>| search.set(e.value().to_string()),
                        }
                        if !suggestions.is_empty() {
                            div { style: "display: flex; flex-direction: column; gap: 2px; max-height: 160px; overflow-y: auto; padding: 6px; margin-top: 4px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 8px;",
                                for slug in suggestions.iter() {
                                    { let s = (*slug).clone();
                                      rsx! {
                                        button {
                                            style: "display: flex; align-items: center; gap: 6px; padding: 5px 8px; background: transparent; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; color: {Theme::TEXT}; text-align: left; width: 100%;",
                                            onclick: { let s = s.clone(); move |_| {
                                                let slug = s.clone();
                                                selected.with_mut(|set| { set.insert(slug); });
                                                search.set(String::new());
                                            }},
                                            span { style: "color: {Theme::ACCENT_STRONG}; font-size: 14px;", "+" }
                                            "{s}"
                                        }
                                    }}
                                }
                            }
                        }
                        if installed.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;", "No installed skills." }
                        }
                    }
                    // Flocks — search-to-add
                    div {
                        p { style: "{label_style}", "{t.preset_flocks}" }
                        if !selected_flocks.read().is_empty() {
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px;",
                                for flock in selected_flocks.read().iter() {
                                    { let slug = flock.clone();
                                      rsx! {
                                        span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: rgba(90, 120, 200, 0.10); color: rgba(50, 80, 160, 0.9); border: 1px solid rgba(90, 120, 200, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                            "{slug}"
                                            button {
                                                style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                                onclick: { let slug = slug.clone(); move |_| { let s = slug.clone(); selected_flocks.with_mut(|set| { set.remove(&s); }); } },
                                                "\u{00D7}"
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        input { r#type: "text", value: "{flock_search}", placeholder: "{t.selectors_search_flocks}",
                            style: "width: 100%; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                            oninput: move |e: Event<FormData>| flock_search.set(e.value().to_string()),
                        }
                        if !flock_suggestions.is_empty() {
                            div { style: "display: flex; flex-direction: column; gap: 2px; max-height: 160px; overflow-y: auto; padding: 6px; margin-top: 4px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 8px;",
                                for slug in flock_suggestions.iter() {
                                    { let s = (*slug).clone();
                                      rsx! {
                                        button {
                                            style: "display: flex; align-items: center; gap: 6px; padding: 5px 8px; background: transparent; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; color: {Theme::TEXT}; text-align: left; width: 100%;",
                                            onclick: { let s = s.clone(); move |_| {
                                                let slug = s.clone();
                                                selected_flocks.with_mut(|set| { set.insert(slug); });
                                                flock_search.set(String::new());
                                            }},
                                            span { style: "color: rgba(50, 80, 160, 0.9); font-size: 14px;", "+" }
                                            "{s}"
                                        }
                                    }}
                                }
                            }
                        }
                        if all_flock_slugs.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;", "No flocks in registry. Sync first." }
                        }
                    }
                    if let Some(ref msg) = *error.read() {
                        p { style: "font-size: 12px; color: {Theme::DANGER};", "{msg}" }
                    }
                    div { style: "display: flex; gap: 10px; justify-content: flex-end;",
                        button { style: "padding: 8px 20px; background: transparent; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer;",
                            onclick: move |_| show.set(false), "{t.close}"
                        }
                        button { style: "padding: 8px 20px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 10px; font-size: 13px; font-weight: 700; cursor: pointer;",
                            onclick: do_save, "{t.profile_create}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PresetCard(
    name: String,
    description: Option<String>,
    skills: Vec<String>,
    mut version: Signal<u32>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut skills_page = use_signal(|| 0usize);
    let mut show_add = use_signal(|| false);

    let preset_name = name.clone();
    let preset_name_del = name.clone();
    let desc_display = description.unwrap_or_default();
    let current_page =
        pagination::clamp_page(*skills_page.read(), skills.len(), PRESET_SKILLS_PAGE_SIZE);
    let visible_skills = pagination::slice_for_page(&skills, current_page, PRESET_SKILLS_PAGE_SIZE);
    let total_pages = pagination::total_pages(skills.len(), PRESET_SKILLS_PAGE_SIZE);

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px;",
            div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;",
                div { style: "display: flex; align-items: center; gap: 10px;",
                    span { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};", "{preset_name}" }
                    if !desc_display.is_empty() {
                        span { style: "font-size: 12px; color: {Theme::MUTED};", "- {desc_display}" }
                    }
                }
                div { style: "display: flex; gap: 6px;",
                    button {
                        style: "padding: 4px 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; font-size: 12px; cursor: pointer;",
                        onclick: move |_| show_add.set(true),
                        "{t.profile_add_skill}"
                    }
                    button {
                        style: "padding: 4px 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; font-size: 12px; cursor: pointer;",
                        onclick: move |_| {
                            let _ = delete_preset(&preset_name_del);
                            version.with_mut(|v| *v += 1);
                        },
                        "{t.profile_delete}"
                    }
                }
            }

            h4 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                "{t.profile_skills} ({skills.len()})"
            }

            if skills.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED};", "{t.profile_no_skills}" }
            } else {
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    for skill in visible_skills.iter() {
                        {
                            let slug = skill.clone();
                            let preset_for_remove = name.clone();
                            rsx! {
                                div { style: "display: flex; align-items: center; gap: 4px; padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; border-radius: 12px;",
                                    span { style: "font-size: 12px; color: {Theme::ACCENT_STRONG}; font-weight: 500;", "{slug}" }
                                    button {
                                        style: "background: none; border: none; color: {Theme::MUTED}; font-size: 14px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                        onclick: move |_| {
                                            let _ = remove_skills_from_preset(&preset_for_remove, &[slug.clone()]);
                                            version.with_mut(|v| *v += 1);
                                        },
                                        "\u{00D7}"
                                    }
                                }
                            }
                        }
                    }
                }
                PaginationControls {
                    current_page: current_page,
                    total_pages: Some(total_pages),
                    has_prev: current_page > 0,
                    has_next: current_page + 1 < total_pages,
                    on_prev: move |_| skills_page.set(current_page.saturating_sub(1)),
                    on_next: move |_| skills_page.set(current_page + 1),
                }
            }
        }

        // Add skills modal
        if *show_add.read() {
            AddSkillsToPresetModal { preset_name: name.clone(), current_skills: skills.clone(), show: show_add, version: version }
        }
    }
}

/// Modal for adding installed skills to an existing preset.
#[component]
fn AddSkillsToPresetModal(
    preset_name: String,
    current_skills: Vec<String>,
    show: Signal<bool>,
    version: Signal<u32>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut backdrop_pressed = use_signal(|| false);
    let mut selected = use_signal(|| BTreeSet::<String>::new());
    let mut search = use_signal(String::new);

    let current_set: BTreeSet<String> = current_skills.into_iter().collect();
    let installed = savhub_local::registry::list_installed_slugs().unwrap_or_default();
    let search_val = search.read().to_lowercase();
    // Only show unselected, not-in-preset skills matching query
    let suggestions: Vec<&String> = if search_val.is_empty() {
        Vec::new()
    } else {
        installed
            .iter()
            .filter(|s| {
                !current_set.contains(*s)
                    && !selected.read().contains(*s)
                    && s.to_lowercase().contains(&search_val)
            })
            .take(20)
            .collect()
    };

    let do_add = move |_| {
        let slugs: Vec<String> = selected.read().iter().cloned().collect();
        if !slugs.is_empty() {
            let _ = add_skills_to_preset(&preset_name, &slugs);
            version.with_mut(|v| *v += 1);
        }
        show.set(false);
    };

    rsx! {
        div {
            style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 1000; display: flex; align-items: center; justify-content: center; padding: 24px;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { show.set(false); } backdrop_pressed.set(false); },
            div {
                style: "background: {Theme::PANEL}; border: 2px solid {Theme::ACCENT}; border-radius: 18px; padding: 28px; width: 500px; max-width: 92vw; max-height: 92vh; overflow-y: auto; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.25);",
                onmousedown: move |evt: Event<MouseData>| evt.stop_propagation(),
                onmouseup: move |evt: Event<MouseData>| evt.stop_propagation(),
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px;",
                    h2 { style: "font-size: 18px; font-weight: 800; color: {Theme::TEXT};", "{t.profile_add_skill}" }
                    button { style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| show.set(false), "\u{00D7}"
                    }
                }
                // Selected tags
                if !selected.read().is_empty() {
                    div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px;",
                        for skill in selected.read().iter() {
                            { let slug = skill.clone();
                              rsx! {
                                span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                    "{slug}"
                                    button {
                                        style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                        onclick: { let slug = slug.clone(); move |_| { let s = slug.clone(); selected.with_mut(|set| { set.remove(&s); }); } },
                                        "\u{00D7}"
                                    }
                                }
                            }}
                        }
                    }
                }
                // Search input
                input { r#type: "text", value: "{search}", placeholder: "Search installed skills...",
                    style: "width: 100%; padding: 8px 12px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 13px; background: white; color: {Theme::TEXT};",
                    oninput: move |e: Event<FormData>| search.set(e.value().to_string()),
                }
                // Suggestions dropdown
                if !suggestions.is_empty() {
                    div { style: "display: flex; flex-direction: column; gap: 2px; max-height: 200px; overflow-y: auto; padding: 6px; margin-top: 4px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 8px; margin-bottom: 10px;",
                        for slug in suggestions.iter() {
                            { let s = (*slug).clone();
                              rsx! {
                                button {
                                    style: "display: flex; align-items: center; gap: 6px; padding: 5px 8px; background: transparent; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; color: {Theme::TEXT}; text-align: left; width: 100%;",
                                    onclick: { let s = s.clone(); move |_| {
                                        let slug = s.clone();
                                        selected.with_mut(|set| { set.insert(slug); });
                                        search.set(String::new());
                                    }},
                                    span { style: "color: {Theme::ACCENT_STRONG}; font-size: 14px;", "+" }
                                    "{s}"
                                }
                            }}
                        }
                    }
                }
                div { style: "display: flex; gap: 10px; justify-content: flex-end; margin-top: 14px;",
                    button { style: "padding: 8px 20px; background: transparent; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer;",
                        onclick: move |_| show.set(false), "{t.close}"
                    }
                    button { style: "padding: 8px 20px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 10px; font-size: 13px; font-weight: 700; cursor: pointer;",
                        onclick: do_add, "{t.profile_add_skill}"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rescan & Apply modal
// ---------------------------------------------------------------------------

#[component]
fn RescanModal(project_path: String, mut show: Signal<bool>, mut version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    let workdir = PathBuf::from(&project_path);
    let scan_result = savhub_local::selectors::run_selectors(&workdir).ok();

    // Collect skills from matched selectors + presets + flocks
    let (matched_signs, preset_signs, flock_signs, skill_signs) = if let Some(ref result) =
        scan_result
    {
        let matched: Vec<String> = result
            .matched
            .iter()
            .map(|m| m.selector.name.clone())
            .collect();
        let presets = result.presets.clone();
        let flocks = result.flocks.clone();

        let mut skills: Vec<String> = result.skills.clone();
        let preset_store = read_presets_store().unwrap_or_default();
        for preset_name in &presets {
            if let Some(preset) = preset_store.presets.get(preset_name) {
                for s in &preset.skills {
                    if !skills.contains(s) {
                        skills.push(s.clone());
                    }
                }
            }
        }
        for flock_sign in &flocks {
            if let Ok(flock_skills) = savhub_local::registry::list_flock_skill_slugs(flock_sign) {
                for s in flock_skills {
                    if !skills.contains(&s) {
                        skills.push(s);
                    }
                }
            }
        }
        (matched, presets, flocks, skills)
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };

    let has_match = !matched_signs.is_empty();

    // Resolve AI agents with checkboxes
    let configured_agents = state.agents.read().clone();
    let all_clients = savhub_local::clients::resolve_clients(&configured_agents);
    let client_names: Vec<(String, String, bool)> = all_clients
        .iter()
        .filter(|c| c.kind.project_skills_dir().is_some())
        .map(|c| (c.kind.as_str().to_string(), c.name.clone(), c.installed))
        .collect();

    let mut agent_checks = use_signal(|| {
        client_names
            .iter()
            .map(|(id, _, installed)| (id.clone(), *installed))
            .collect::<Vec<(String, bool)>>()
    });
    let mut apply_status = use_signal(|| 0u8); // 0=idle, 1=applying, 2=done

    let project_path_clone = project_path.clone();
    let preset_signs_cl = preset_signs.clone();
    let skill_signs_cl = skill_signs.clone();
    let do_apply = move |_| {
        apply_status.set(1);
        let workdir = PathBuf::from(&project_path_clone);

        let checked: Vec<String> = agent_checks
            .read()
            .iter()
            .filter(|(_, on)| *on)
            .map(|(id, _)| id.clone())
            .collect();

        if has_match {
            if let Ok(mut cfg) = savhub_local::presets::read_project_config(&workdir) {
                cfg.presets.matched = preset_signs_cl.clone();
                cfg.selectors.matched = scan_result
                    .as_ref()
                    .unwrap()
                    .matched
                    .iter()
                    .map(|m| savhub_local::presets::ProjectSelectorMatch {
                        selector: m.selector.name.clone(),
                        presets: m.presets.clone(),
                        flocks: m.flocks.clone(),
                        skills: m.skills.clone(),
                    })
                    .collect();
                let _ = savhub_local::presets::write_project_config(&workdir, &cfg);
            }

            let config = savhub_local::presets::read_project_config(&workdir).unwrap_or_default();
            let skipped = &config.skills.manual_skipped;
            let filtered: Vec<String> = skill_signs_cl
                .iter()
                .filter(|s| !savhub_local::registry::skill_matches_skipped(s, skipped))
                .cloned()
                .collect();

            if let Ok(results) = savhub_local::registry::install_skills_batch(&filtered) {
                let agents = savhub_local::clients::resolve_clients(&checked);
                for info in &results {
                    for client in &agents {
                        if !client.installed {
                            continue;
                        }
                        let Some(rel_dir) = client.kind.project_skills_dir() else {
                            continue;
                        };
                        let target = workdir.join(rel_dir).join(&info.slug);
                        let _ = std::fs::create_dir_all(target.parent().unwrap());
                        let _ = savhub_local::skills::copy_skill_folder(&info.local_path, &target);
                    }
                }
                let mut lock =
                    savhub_local::presets::read_project_lockfile(&workdir).unwrap_or_default();
                for info in &results {
                    if !lock.skills.iter().any(|s| s.slug() == info.slug) {
                        let vi = savhub_local::skills::read_skill_version_info(&info.local_path)
                            .unwrap_or_default();
                        lock.skills.push(savhub_local::presets::ProjectLockedSkill {
                            repo: info.repo_sign.clone(),
                            path: info.skill_path.clone(),
                            version: vi.version,
                            commit_hash: vi.git_commit,
                        });
                    }
                }
                let _ = savhub_local::presets::write_project_lockfile(&workdir, &lock);
            }
        } else {
            let lock = savhub_local::presets::read_project_lockfile(&workdir).unwrap_or_default();
            let agents = savhub_local::clients::resolve_clients(&checked);
            for skill in &lock.skills {
                for client in &agents {
                    if !client.installed {
                        continue;
                    }
                    let Some(rel_dir) = client.kind.project_skills_dir() else {
                        continue;
                    };
                    let _ = std::fs::remove_dir_all(workdir.join(rel_dir).join(skill.slug()));
                }
            }
            if let Ok(mut cfg) = savhub_local::presets::read_project_config(&workdir) {
                cfg.selectors.matched.clear();
                cfg.presets.matched.clear();
                cfg.presets.manual_added.clear();
                cfg.presets.manual_skipped.clear();
                let _ = savhub_local::presets::write_project_config(&workdir, &cfg);
            }
            let _ = std::fs::remove_file(workdir.join("savhub.lock"));
        }

        let _ = savhub_local::config::add_project(&workdir.display().to_string());
        apply_status.set(2);
        version.with_mut(|v| *v += 1);
    };

    let status = *apply_status.read();

    rsx! {
        div { style: "position: fixed; inset: 0; background: rgba(0,0,0,0.35); display: flex; align-items: center; justify-content: center; z-index: 100;",
            onclick: move |_| { if status != 1 { show.set(false); } },
            div {
                style: "background: {Theme::BG_ELEVATED}; border-radius: 16px; padding: 28px 32px; min-width: 480px; max-width: 600px; max-height: 80vh; overflow-y: auto; box-shadow: 0 8px 32px rgba(0,0,0,0.18);",
                onclick: move |e| e.stop_propagation(),

                h2 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 16px;",
                    "{t.project_rescan_title}"
                }

                if !has_match {
                    p { style: "font-size: 14px; color: {Theme::MUTED}; margin-bottom: 16px;",
                        "{t.project_rescan_no_match}"
                    }
                } else {
                    div { style: "margin-bottom: 14px;",
                        h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 6px;",
                            "{t.project_rescan_matched} ({matched_signs.len()})"
                        }
                        for name in matched_signs.iter() {
                            div { style: "padding: 4px 0; font-size: 13px; color: {Theme::ACCENT_STRONG};",
                                "\u{2713} {name}"
                            }
                        }
                    }

                    if !preset_signs.is_empty() {
                        div { style: "margin-bottom: 14px;",
                            h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 6px;",
                                "{t.project_rescan_presets}"
                            }
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                                for p in preset_signs.iter() {
                                    span { style: "padding: 3px 10px; background: {Theme::ACCENT_LIGHT}; border-radius: 12px; font-size: 12px; color: {Theme::ACCENT_STRONG};",
                                        "{p}"
                                    }
                                }
                            }
                        }
                    }

                    if !flock_signs.is_empty() {
                        div { style: "margin-bottom: 14px;",
                            h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 6px;",
                                "{t.project_rescan_flocks}"
                            }
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                                for f in flock_signs.iter() {
                                    span { style: "padding: 3px 10px; background: {Theme::ACCENT_LIGHT}; border-radius: 12px; font-size: 12px; color: {Theme::ACCENT_STRONG};",
                                        "{f}"
                                    }
                                }
                            }
                        }
                    }

                    if !skill_signs.is_empty() {
                        div { style: "margin-bottom: 14px;",
                            h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 6px;",
                                "{t.project_rescan_skills} ({skill_signs.len()})"
                            }
                            div { style: "max-height: 120px; overflow-y: auto;",
                                for s in skill_signs.iter() {
                                    div { style: "padding: 2px 0; font-size: 13px; color: {Theme::TEXT};",
                                        "{s}"
                                    }
                                }
                            }
                        }
                    }
                }

                // AI Agents checkboxes
                div { style: "margin-bottom: 16px; padding-top: 10px; border-top: 1px solid {Theme::LINE};",
                    h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 8px;",
                        "{t.settings_agents}"
                    }
                    for (idx , (_id , display_name , _installed)) in client_names.iter().enumerate() {
                        {
                            let checked = agent_checks.read().get(idx).map(|(_, v)| *v).unwrap_or(false);
                            let idx_copy = idx;
                            rsx! {
                                label { style: "display: flex; align-items: center; gap: 8px; padding: 4px 0; font-size: 13px; color: {Theme::TEXT}; cursor: pointer;",
                                    input {
                                        r#type: "checkbox",
                                        checked: checked,
                                        onchange: move |e: Event<FormData>| {
                                            let val = e.value() == "true";
                                            agent_checks.with_mut(|list| {
                                                if let Some(entry) = list.get_mut(idx_copy) {
                                                    entry.1 = val;
                                                }
                                            });
                                        },
                                    }
                                    "{display_name}"
                                }
                            }
                        }
                    }
                }

                // Action buttons
                div { style: "display: flex; gap: 10px; justify-content: flex-end;",
                    button {
                        style: "padding: 8px 20px; background: transparent; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer;",
                        disabled: status == 1,
                        onclick: move |_| show.set(false),
                        "{t.project_rescan_close}"
                    }
                    if status == 0 {
                        button {
                            style: "padding: 8px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 10px; font-size: 13px; font-weight: 700; cursor: pointer;",
                            onclick: do_apply,
                            "{t.project_rescan_apply}"
                        }
                    }
                    if status == 1 {
                        span { style: "padding: 8px 12px; font-size: 13px; color: {Theme::MUTED};",
                            "{t.project_rescan_applying}"
                        }
                    }
                    if status == 2 {
                        span { style: "padding: 8px 12px; font-size: 13px; color: {Theme::ACCENT_STRONG}; font-weight: 600;",
                            "{t.project_rescan_done}"
                        }
                    }
                }
            }
        }
    }
}
