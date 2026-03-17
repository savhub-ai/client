use std::collections::BTreeSet;
use std::path::PathBuf;

use dioxus::prelude::*;
use savhub_local::config::{add_project, read_projects_list, remove_project};
use savhub_local::presets::{
    EnableProjectRepoSkillResult, ProjectSkillConflict, ProjectSkillConflictChoice,
    ResolvedSkillSources, disable_project_skill, enable_repo_skill_in_project, list_repo_skills,
    read_project_selector_matches, resolve_project_skills_with_sources,
};

use crate::components::pagination::{self, PaginationControls};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const PROJECTS_PAGE_SIZE: usize = 10;
const PROJECT_SELECTORS_PAGE_SIZE: usize = 8;
const PROJECT_SKILLS_PAGE_SIZE: usize = 8;
const LOCAL_SKILLS_PAGE_SIZE: usize = 10;

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
fn ProjectDetail(project_path: String, mut version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut show_add_skill_dialog = use_signal(|| false);
    let mut show_rescan_modal = use_signal(|| false);
    let mut selectors_page = use_signal(|| 0usize);
    let mut effective_skills_page = use_signal(|| 0usize);

    let _ = *version.read();

    let workdir = PathBuf::from(&project_path);
    let selector_matches = read_project_selector_matches(&workdir).unwrap_or_default();
    let effective_skills = resolve_project_skills_with_sources(&workdir).unwrap_or_default();
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
                enabled_label: t.installed,
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
fn SelectorMatchRow(selector: String) -> Element {
    rsx! {
        div { style: "padding: 12px 14px; background: {Theme::BG_ELEVATED}; border: 1px solid {Theme::LINE}; border-radius: 8px;",
            p { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT};",
                "{selector}"
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

// ---------------------------------------------------------------------------
// Rescan & Apply modal
// ---------------------------------------------------------------------------

#[component]
fn RescanModal(project_path: String, mut show: Signal<bool>, mut version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    let workdir = PathBuf::from(&project_path);
    let scan_result = savhub_local::selectors::run_selectors(&workdir).ok();

    // Collect skills from matched selectors + flocks
    let (matched_signs, flock_signs, skill_signs) = if let Some(ref result) = scan_result {
        let matched: Vec<String> = result
            .matched
            .iter()
            .map(|m| m.selector.name.clone())
            .collect();
        let flocks = result.flocks.clone();

        let mut skills: Vec<String> = result.skills.clone();
        for flock_sign in &flocks {
            if let Ok(flock_skills) = savhub_local::registry::list_flock_skill_slugs(flock_sign) {
                for s in flock_skills {
                    if !skills.contains(&s) {
                        skills.push(s);
                    }
                }
            }
        }
        (matched, flocks, skills)
    } else {
        (Vec::new(), Vec::new(), Vec::new())
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
                cfg.selectors.matched = scan_result
                    .as_ref()
                    .unwrap()
                    .matched
                    .iter()
                    .map(|m| savhub_local::presets::ProjectSelectorMatch {
                        selector: m.selector.name.clone(),
                        flocks: m.flocks.clone(),
                        skills: m.skills.clone(),
                        repos: m.repos.clone(),
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
                            sign: savhub_local::registry::make_skill_sign(
                                &info.repo_sign,
                                &info.skill_path,
                            ),
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
