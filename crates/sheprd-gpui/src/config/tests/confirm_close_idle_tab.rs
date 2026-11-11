use super::*;

#[test]
fn confirm_close_idle_tab_defaults_off_and_is_editable_from_the_cli() -> anyhow::Result<()> {
    assert!(!Config::default().confirm_close_idle_tab);
    assert!(!Config::parse("")?.confirm_close_idle_tab);
    assert!(!Config::parse(DEFAULT_CONFIG)?.confirm_close_idle_tab);
    let config = Config::parse("confirm_close_idle_tab = true")?;
    assert!(config.confirm_close_idle_tab);
    // It adds to the tab option rather than replacing it.
    assert!(config.confirm_close_tab);
    assert!(Config::parse("confirm_close_idle_tab = 'true'").is_err());
    assert!(Config::parse("confirm_close_idle_tab = 1").is_err());
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = temp.0.join("absent.toml");
    let original = "confirm_close_idle_tab = false # keep\n[terminal]\nsize = 16\n";
    fs::write(&local, original)?;
    Config::edit_key_at(
        &local,
        &daemon,
        "confirm_close_idle_tab",
        Some(parse_value("true")),
    )?;
    assert_eq!(
        fs::read_to_string(&local)?,
        original.replace("false", "true")
    );
    assert_eq!(
        Config::effective_value_at(&path, "confirm_close_idle_tab")?,
        Some((toml::Value::Boolean(true), Layer::Local))
    );
    assert!(
        Config::edit_key_at(
            &local,
            &daemon,
            "confirm_close_idle_tab",
            Some(parse_value("1"))
        )
        .is_err()
    );
    Ok(())
}
