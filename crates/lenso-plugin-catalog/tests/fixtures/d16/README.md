# D16 signed contract fixtures

These are test-only signed envelopes. The fixed Ed25519 seed is `[7; 32]`,
its public key is `ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c`,
the key ID and catalog ID are both `key` and `catalog` respectively, and
snapshots are current only from Unix time 103 through 199. No archive or
documentation URL here is a live publication. The reference byte strings used
for metadata digests are not adoption artifacts.

`package-snapshot.envelope.json` supplies the signed npm-only base for
`package-content.envelope.json`; the package has a versioned Markdown reference.
`content-only.envelope.json` has no portable, Cargo, or npm base. Its
`base_release_identity` is the SHA-256 of UTF-8 JSON bytes of
`[plugin_id, version, metadata_tuple, [[id, kind, url, digest, size], ...]]`
in signed content array order. The metadata tuple binds publisher, title,
summary, exact source, license and versioned Markdown. That identity is
`sha256:cf0b3c4772493e69571cce016a52fcabf545d030011ed442e59de490a5efb721`.
The npm-only base identity is
`sha256:e4d3041c85808055ca7141472e4b04faade1663756819413e4d9f645999d1121`.

The signatures cover the exact decoded payload bytes with the corresponding
schema domain separator, followed by `key`, NUL, and the payload. These files
test cross-language signature and identity joins only; they do not prove the
example URLs serve the referenced bytes.
`getting-started.md` is the exact 15-byte Markdown body for both signed
documentation references and can be served by a loopback-only test origin.
The content-only fixture uses `example.editor.source@1.0.0`, distinct from
the package base's `example.editor@1.0.0`, so it is a positive sample under
the cross-channel identity collision rule.
