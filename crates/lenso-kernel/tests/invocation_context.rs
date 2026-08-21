use lenso_kernel::{
    CancellationToken, InvocationContext, InvocationContextError, SealedInvocationExtension,
};

#[test]
fn sealed_extensions_are_opaque_and_cannot_be_overwritten() {
    let context = InvocationContext::new(7, None, CancellationToken::new());
    let assertion = SealedInvocationExtension::signed(
        "lenso.auth.actor-assertion",
        "auth.users",
        ["example.secure-greeting@1:greet"],
        br#"{"subject":"user-123"}"#.to_vec(),
        "signed-proof",
    );
    let context = context
        .with_sealed_extension(assertion.clone())
        .expect("the first sealed extension should be accepted");

    assert_eq!(context.sealed_extension(assertion.key()), Some(&assertion));
    assert_eq!(context.sealed_extensions().count(), 1);

    let replacement = SealedInvocationExtension::signed(
        assertion.key(),
        "forged-issuer",
        ["example.secure-greeting@1:greet"],
        br#"{"subject":"attacker"}"#.to_vec(),
        "forged-proof",
    );
    assert!(matches!(
        context.clone().with_sealed_extension(replacement),
        Err(InvocationContextError::SealedExtensionAlreadySet { key })
            if key == assertion.key()
    ));
    assert!(matches!(
        context
            .clone()
            .with_extension(assertion.key(), br#"{"subject":"attacker"}"#.to_vec()),
        Err(InvocationContextError::SealedExtensionAlreadySet { key })
            if key == assertion.key()
    ));
    assert!(matches!(
        InvocationContext::new(8, None, CancellationToken::new()).with_sealed_extension(
            SealedInvocationExtension::signed(
                "invalid",
                "auth.users",
                [""],
                Vec::new(),
                "proof",
            )
        ),
        Err(InvocationContextError::InvalidSealedExtension { key }) if key == "invalid"
    ));
    assert!(matches!(
        InvocationContext::new(9, None, CancellationToken::new()).with_sealed_extension(
            SealedInvocationExtension::signed(
                "invalid",
                "auth.users",
                ["example.secure-greeting@1:greet"],
                Vec::new(),
                "",
            )
        ),
        Err(InvocationContextError::InvalidSealedExtension { key }) if key == "invalid"
    ));

    let debug = format!("{context:?}");
    assert!(!debug.contains("user-123"));
    assert!(!debug.contains("signed-proof"));
}

#[test]
fn ordinary_extensions_are_kept_separate_from_caller_identity() {
    let context = InvocationContext::new(11, None, CancellationToken::new())
        .with_caller_instance("ingress")
        .with_extension("traceparent", b"00-abc".to_vec())
        .expect("ordinary extensions should be accepted");

    assert_eq!(context.caller_instance(), Some("ingress"));
    assert_eq!(context.extension("traceparent"), Some(&b"00-abc"[..]));
    assert!(context.sealed_extension("traceparent").is_none());
}

#[test]
fn sealed_extensions_only_propagate_to_covered_capability_operations() {
    let context = InvocationContext::new(12, None, CancellationToken::new())
        .with_sealed_extension(SealedInvocationExtension::signed(
            "lenso.auth.actor-assertion",
            "auth.users",
            ["example.secure-greeting@1:greet"],
            br#"{"subject":"user-123"}"#.to_vec(),
            "signed-proof",
        ))
        .expect("the signed assertion should attach")
        .with_extension("traceparent", b"00-abc".to_vec())
        .expect("ordinary baggage should attach");

    let covered = context
        .clone()
        .for_target("example.secure-greeting@1", "greet");
    assert!(
        covered
            .sealed_extension("lenso.auth.actor-assertion")
            .is_some()
    );
    assert_eq!(covered.extension("traceparent"), Some(&b"00-abc"[..]));

    let unrelated = context.for_target("example.profile@1", "read");
    assert!(
        unrelated
            .sealed_extension("lenso.auth.actor-assertion")
            .is_none()
    );
    assert_eq!(unrelated.extension("traceparent"), Some(&b"00-abc"[..]));
}
