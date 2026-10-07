use std::collections::HashSet;

use dji4g_domain::{DevicePresence, DeviceProfile, QUECTEL_GENERIC};
use dji4g_panel::localization::{
    Language, TextKey, available_languages, device_model_name, device_presence, language_key,
    limited_reason, stable_code_text, template,
};

/// Keys whose text is deliberately not Chinese: a language names itself in its own script, so the
/// English endonym stays "English" in the simplified catalog too.
///
/// `SmsFailureWithCode` and `ComposeCmsError` are punctuation and a protocol acronym around a `{}`
/// slot: the sentence they hang off is a key of its own (`SmsErr*`, `ComposeFailure*`), and the only
/// localized decision they make is the pair of brackets — which is exactly the full-width/ASCII
/// difference a translation has to be able to make.
const NOT_EXPECTED_TO_BE_CHINESE: &[TextKey] = &[
    TextKey::LanguageEnUs,
    TextKey::SmsFailureWithCode,
    TextKey::ComposeCmsError,
];

#[test]
fn catalog_keys_are_unique_and_have_simplified_chinese_text() {
    let unique = TextKey::ALL.iter().copied().collect::<HashSet<_>>();
    assert_eq!(unique.len(), TextKey::ALL.len());

    for key in TextKey::ALL {
        let value = template(Language::ZhCn, *key);
        assert!(!value.trim().is_empty(), "empty text for {key:?}");
        assert!(!value.contains("TODO"));
        assert!(!value.contains("TBD"));
        if NOT_EXPECTED_TO_BE_CHINESE.contains(key) {
            continue;
        }
        assert!(
            value
                .chars()
                .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)),
            "text for {key:?} is not zh-CN: {value}"
        );
    }
}

/// The picker offers exactly the languages the panel ships, and each option is named in its own
/// script in every catalog — someone who cannot read the current language must still be able to find
/// theirs in the list.
#[test]
fn the_language_selector_offers_every_shipped_language() {
    assert_eq!(
        available_languages(),
        &[Language::ZhCn, Language::ZhTw, Language::EnUs]
    );
    assert_eq!(
        template(Language::EnUs, language_key(Language::EnUs)),
        "English"
    );
    assert_eq!(
        template(Language::ZhTw, language_key(Language::ZhCn)),
        "简体中文"
    );
    assert_eq!(
        template(Language::ZhTw, language_key(Language::ZhTw)),
        "繁體中文"
    );
}

#[test]
fn known_backend_code_namespaces_have_a_localized_fallback() {
    let codes = [
        "apn:empty",
        "at_protocol:wrong_port_data",
        "pdp_context_id:out_of_range",
        "net:adapter_identity_mismatch",
        "net:route_enumeration_failed",
        "pnp:property_invalid",
        "pnp:registry_value_invalid",
        "probe:bind_failed",
        "probe:connect_failed",
        "probe:dns_timeout",
        "probe:http_invalid",
        "probe:tls_timeout",
        "app:adapter_not_ready",
        "app:missing_before_state",
        "at:future",
        "operation:uac_cancelled",
        "route:not_observed",
        "serial_actor:closed",
        "export:write_failed",
        "export:path_unavailable",
        "privilege:helper_unsigned",
        "privilege:helper_unverified",
        "sms:pdu_mode_required",
        "sms:pdu_confirm_failed",
        "sms:invalid_message",
        "sms:send_failed",
        "sms:timeout",
        "sms:device_removed",
        "sms:unsupported",
        "sms:verification_failed",
        "sms:internal",
        "sms:send_unavailable",
        "archive:crypto_unsupported",
        "archive:crypto_too_large",
        "archive:crypto_failed",
        "archive:crypto_decrypt_failed",
        "driver:ready",
        "driver:restart_required",
        "driver:restart_after_failure",
        "driver:cancelled",
        "driver:no_match",
        "driver:interfaces_abnormal",
        "driver:no_module",
        "driver:payload_invalid",
        "driver:incomplete",
        "driver:windows_update_failed",
        "driver:admin_required",
        "driver:panel_not_same_directory",
        "driver:panel_exit_timeout",
    ];

    for code in codes {
        assert!(
            stable_code_text(code).is_some(),
            "unlocalized stable code: {code}"
        );
    }
    assert_eq!(stable_code_text("future_component:new_code"), None);
    assert_eq!(stable_code_text("driver:future_code"), None);
}

#[test]
fn sms_stable_codes_map_to_precise_operator_notes() {
    let cases = [
        ("sms:pdu_mode_required", "PDU"),
        ("sms:pdu_confirm_failed", "未能确认"),
        ("sms:invalid_message", "收件人"),
        ("sms:send_failed", "模块拒绝"),
        ("sms:timeout", "超时"),
        ("sms:device_removed", "断开"),
        ("sms:unsupported", "不支持"),
        ("sms:verification_failed", "格式"),
        ("sms:internal", "内部错误"),
    ];
    for (code, needle) in cases {
        let key =
            stable_code_text(code).unwrap_or_else(|| panic!("unlocalized stable code: {code}"));
        let text = template(Language::ZhCn, key);
        assert!(text.contains(needle), "{code} => {text:?} lacks {needle:?}");
    }
    // Unknown codes under the namespace keep a localized generic fallback.
    let fallback = stable_code_text("sms:future_code").expect("sms namespace fallback");
    assert_eq!(template(Language::ZhCn, fallback), "短信操作失败。");
}

/// The read-only generic module speaks in its own words in all three catalogs, and the refusal a
/// write command produces is that same sentence rather than a generic failure.
#[test]
fn generic_module_wording_is_localized_in_every_catalog() {
    assert_eq!(
        stable_code_text("app:read_only_module"),
        Some(TextKey::ReadOnlyModuleReason)
    );
    assert_eq!(
        limited_reason(dji4g_domain::LimitedReason::ReadOnlyModule),
        TextKey::ReadOnlyModuleReason
    );

    for language in [Language::ZhCn, Language::ZhTw, Language::EnUs] {
        for key in [
            TextKey::ReadOnlyModuleReason,
            TextKey::DeviceModelNameQuectelGeneric,
            TextKey::DevicePresenceSupportedQuectelGeneric,
        ] {
            let text = template(language, key);
            assert!(
                !text.trim().is_empty(),
                "empty {language:?} text for {key:?}"
            );
            assert!(!text.contains("TODO"), "{language:?} {key:?} is unfinished");
        }
    }

    // The DJI wording is byte-for-byte what it always was.
    assert_eq!(
        template(Language::ZhCn, TextKey::DeviceModelName),
        "DJI 一代 4G 模块"
    );
    assert_eq!(
        template(Language::ZhCn, TextKey::DevicePresenceSupported),
        "已检测到受支持的 DJI 一代 4G 模块"
    );
    assert_eq!(
        template(Language::EnUs, TextKey::DeviceModelName),
        "DJI 1st-gen 4G module"
    );

    // The English catalog never falls back to the Chinese sentence for the new keys.
    for key in [
        TextKey::ReadOnlyModuleReason,
        TextKey::DeviceModelNameQuectelGeneric,
        TextKey::DevicePresenceSupportedQuectelGeneric,
    ] {
        assert_ne!(
            template(Language::EnUs, key),
            template(Language::ZhCn, key),
            "{key:?} was left in Chinese in the English catalog"
        );
    }
}

/// Every profile-aware label resolves to the profile that actually matched, and a profile the panel
/// may write to never borrows the generic module's wording.
#[test]
fn module_labels_follow_the_matched_profile() {
    assert_eq!(
        device_model_name(QUECTEL_GENERIC),
        TextKey::DeviceModelNameQuectelGeneric
    );
    assert_eq!(
        device_model_name(DeviceProfile::DJI_GEN1),
        TextKey::DeviceModelName
    );
    assert_eq!(
        device_presence(&DevicePresence::Supported(QUECTEL_GENERIC)),
        TextKey::DevicePresenceSupportedQuectelGeneric
    );
    assert_eq!(
        device_presence(&DevicePresence::Supported(DeviceProfile::DJI_GEN1)),
        TextKey::DevicePresenceSupported
    );
    assert_eq!(
        device_presence(&DevicePresence::Unsupported {
            vid: 0x2CA3,
            pid: 0x4009
        }),
        TextKey::DevicePresenceUnsupported
    );

    // Only the write-capable profile may claim full-support wording.
    assert!(
        !template(
            Language::ZhCn,
            device_presence(&DevicePresence::Supported(QUECTEL_GENERIC))
        )
        .contains("DJI")
    );
}
