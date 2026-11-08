//! In-app reporting to a public Jabu Host.

use dioxus::prelude::*;
use xmpp_core::{PubSubItem, XmppClient};

use crate::{locale, ClientState};

/// Moderation target for one feed or community item.
#[must_use]
pub fn target_for_item(client: &XmppClient, item: &PubSubItem) -> Option<String> {
    if item.node_id == xmpp_core::MICROBLOG_NODE {
        return Some(format!("post/{}/{}", item.publisher.to_bare(), item.id));
    }
    client
        .pubsub
        .communities
        .list()
        .into_iter()
        .find(|community| community.id.node == item.node_id)
        .map(|community| {
            format!(
                "community/{}/{}/{}",
                community.id.service, community.id.node, item.id
            )
        })
}

/// Moderation target for one Story item.
#[must_use]
pub fn target_for_story(item: &PubSubItem) -> String {
    format!("story/{}/{}", item.publisher.to_bare(), item.id)
}

/// Modal form that submits a report to the configured Host.
#[component]
pub fn ReportDialog(target: String, onclose: EventHandler<()>) -> Element {
    let i18n = locale::use_i18n();
    let client_state = use_context::<ClientState>();
    let host = client_state.report_host.read().clone();
    let mut reason = use_signal(|| "spam".to_string());
    let mut details = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut receipt = use_signal(|| None::<(String, String)>);

    let submit_target = target.clone();
    let submit_host = host.clone();
    let submit = move |_| {
        if receipt.read().is_some() {
            return;
        }
        let Some(host) = submit_host.clone() else {
            error.set(Some(i18n.t("report-no-host")));
            return;
        };
        let details = details.read().trim().to_string();
        let request = platform::report::ReportRequest {
            target: submit_target.clone(),
            reason: reason.read().clone(),
            details: (!details.is_empty()).then_some(details),
            reporter: None,
        };
        sending.set(true);
        error.set(None);
        spawn(async move {
            match platform::report::submit_report(&host, &request).await {
                Ok(value) => {
                    receipt.set(Some((value.id, value.status)));
                    error.set(None);
                }
                Err(err) => error.set(Some(err.to_string())),
            }
            sending.set(false);
        });
    };

    let dialog = crate::a11y::use_modal(move || onclose.call(()));
    let mut report_args = locale::Args::new();
    report_args.set(
        "id",
        receipt
            .read()
            .as_ref()
            .map(|(id, _)| id.clone())
            .unwrap_or_default(),
    );
    report_args.set(
        "status",
        receipt
            .read()
            .as_ref()
            .map(|(_, status)| status.clone())
            .unwrap_or_default(),
    );
    rsx! {
        div { class: "fixed inset-0 z-[80] flex items-center justify-center bg-black/70 p-4",
            div {
                id: "{dialog.id}",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": {i18n.t("report-title")},
                "tabindex": "-1",
                onkeydown: dialog.escape(),
                class: "w-full max-w-md space-y-3 rounded-xl border border-gray-700 bg-gray-800 p-5",
                div { class: "flex items-start justify-between gap-3",
                    h2 { class: "text-lg font-semibold text-gray-100", {i18n.t("report-title")} }
                    button {
                        class: "text-xl leading-none text-gray-400 hover:text-gray-100",
                        title: {i18n.t("common-close")},
                        "aria-label": {i18n.t("common-close")},
                        onclick: move |_| onclose.call(()),
                        "×"
                    }
                }
                p { class: "text-[11px] uppercase tracking-wide text-gray-500", {i18n.t("report-target")} }
                code { class: "block break-all rounded-lg bg-gray-900 px-2 py-1 text-xs text-gray-300",
                    "{target}"
                }
                if host.is_none() {
                    p { class: "rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-200",
                        {i18n.t("report-no-host")}
                    }
                }
                if let Some((id, status)) = receipt.read().clone() {
                    p { class: "rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200",
                        "{i18n.t_args(\"report-stored\", &report_args)}"
                    }
                    div { class: "flex justify-end",
                        button {
                            class: "rounded-lg bg-teal-500 px-3 py-1.5 text-sm text-white hover:bg-teal-400",
                            onclick: move |_| onclose.call(()),
                            {i18n.t("common-close")}
                        }
                    }
                } else {
                    label { class: "block text-xs text-gray-400",
                        {i18n.t("report-reason")}
                        select {
                            class: "mt-1 w-full rounded-lg border border-gray-600 bg-gray-700 px-2 py-1.5 text-sm text-gray-100",
                            value: "{reason}",
                            onchange: move |event| reason.set(event.value()),
                            option { value: "spam", {i18n.t("report-spam")} }
                            option { value: "abuse", {i18n.t("report-abuse")} }
                            option { value: "illegal", {i18n.t("report-illegal")} }
                            option { value: "other", {i18n.t("report-other")} }
                        }
                    }
                    label { class: "block text-xs text-gray-400",
                        {i18n.t("report-details")}
                        textarea {
                            class: "mt-1 w-full rounded-lg border border-gray-600 bg-gray-700 px-2 py-1.5 text-sm text-gray-100",
                            rows: 3,
                            maxlength: 2000,
                            value: "{details}",
                            oninput: move |event| details.set(event.value()),
                        }
                    }
                    if let Some(err) = error.read().as_ref() {
                        p { class: "text-xs text-red-400", "{err}" }
                    }
                    div { class: "flex items-center justify-end gap-2",
                        button {
                            class: "rounded-lg bg-gray-700 px-3 py-1.5 text-sm text-gray-100 hover:bg-gray-600",
                            onclick: move |_| onclose.call(()),
                            {i18n.t("common-cancel")}
                        }
                        button {
                            class: "rounded-lg bg-red-600 px-3 py-1.5 text-sm text-white hover:bg-red-500 disabled:opacity-50",
                            disabled: *sending.read() || host.is_none(),
                            onclick: submit,
                            if *sending.read() { {i18n.t("report-sending")} } else { {i18n.t("report-send")} }
                        }
                    }
                }
            }
        }
    }
}
