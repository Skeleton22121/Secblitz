use super::*;
use base64::Engine as _;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Discover,
    Download,
    Install,
    Verify,
}
pub(super) fn acknowledged(bytes: &[u8]) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ack {
        acknowledged: bool,
    }
    ensure!(bytes.len() <= 128, "Acknowledgement exceeds cap");
    ensure!(
        serde_json::from_slice::<Ack>(bytes)?.acknowledged,
        "Patching command not acknowledged"
    );
    Ok(())
}
pub(super) fn build(
    action: Action,
    plan: Option<&Plan>,
    approval: Option<&Approval>,
) -> Result<String> {
    let action = match action {
        Action::Discover => "discover",
        Action::Download => "download",
        Action::Install => "install",
        Action::Verify => "verify",
    };
    ensure!(
        (action == "discover") == plan.is_none(),
        "Invalid compiled action/plan"
    );
    ensure!(
        matches!(action, "download" | "install") == approval.is_some(),
        "Invalid compiled action/permit"
    );
    // Preserve typed field order for the PowerShell ordered metadata comparison.
    // serde_json::Value would reorder nested objects alphabetically.
    #[derive(Serialize)]
    struct Input<'a> {
        action: &'a str,
        plan: Option<&'a Plan>,
        not_before: u64,
        expires_at: u64,
    }
    let data = serde_json::to_vec(&Input {
        action,
        plan,
        not_before: approval.map_or(0, |a| a.approved_at),
        expires_at: approval.map_or(0, |a| a.expires_at),
    })?;
    ensure!(
        data.len() <= MAX_STATE_BYTES,
        "Patching request cap exceeded"
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let platform = include_str!("../platform/backend.ps1");
    let boundary = "\ntry {\n    switch -CaseSensitive ($action) {";
    ensure!(
        platform.matches(boundary).count() == 1,
        "Policy helper boundary changed"
    );
    let definitions = platform
        .split_once(boundary)
        .context("Missing compiled policy helpers")?
        .0;
    Ok(format!("$inputJson=$null\n{definitions}\n$request=ConvertFrom-Json -InputObject ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')))\n{}", include_str!("wua.ps1")))
}
