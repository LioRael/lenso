#[test]
fn architecture_rules_pass_for_current_workspace() {
    arch_check::run().expect("architecture rules should pass");
}
