use lenso_kernel::{
    CancellationToken, InvocationContext, InvocationContextError, SealedInvocationExtension,
};

#[test]
fn sealed_extensions_are_opaque_and_cannot_be_overwritten() {
    let context = InvocationContext::new(7, None, CancellationToken::new());
    let assertion = SealedInvocationExtension::new(
        "lenso.auth.actor-assertion",
        "auth.users",
        ["example.secure-greeting@1:greet"],
        br#"{"subject":"user-123"}"#.to_vec(),
    );
    let context = context
        .with_sealed_extension(assertion.clone())
        .expect("the first sealed extension should be accepted");

    assert_eq!(context.sealed_extension(assertion.key()), Some(&assertion));
    assert_eq!(context.sealed_extensions().count(), 1);

    let replacement = SealedInvocationExtension::new(
        assertion.key(),
        "forged-issuer",
        ["example.secure-greeting@1:greet"],
        br#"{"subject":"attacker"}"#.to_vec(),
    );
    assert!(matches!(
        context.clone().with_sealed_extension(replacement),
        Err(InvocationContextError::SealedExtensionAlreadySet { key })
            if key == assertion.key()
    ));
    assert!(matches!(
        context.with_extension(assertion.key(), br#"{"subject":"attacker"}"#.to_vec()),
        Err(InvocationContextError::SealedExtensionAlreadySet { key })
            if key == assertion.key()
    ));
    assert!(matches!(
        InvocationContext::new(8, None, CancellationToken::new()).with_sealed_extension(
            SealedInvocationExtension::new("invalid", "auth.users", [""], Vec::new())
        ),
        Err(InvocationContextError::InvalidSealedExtension { key }) if key == "invalid"
    ));
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
