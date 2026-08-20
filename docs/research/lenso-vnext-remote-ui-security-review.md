# Lenso vNext remote UI security review

## Scope

This note evaluates browser extension mechanisms for Lenso Console, with particular attention to the request to allow custom pages from arbitrary URLs without granting those pages ambient Console authority. It uses browser standards and official platform documentation as its evidence base. It does not decide the overall Console deployment architecture.

The central conclusion is:

> An arbitrary URL may be embedded only as a low-trust, sandboxed external page. It must not be imported as JavaScript into the Console realm. Same-realm UI code must come from a trusted, locally installed and locked bundle.

This requires two explicit trust classes rather than one universal UI Contribution execution model:

1. **Trusted UI Contribution** — locally installed, version-locked code that may execute in the Console realm.
2. **Sandboxed External Page** — mutable remote content in an isolated iframe that receives only explicitly brokered operations.

## Decision outcome

ADR [0043](../adr/0043-represent-ui-contributions-as-capabilities.md)
subsequently chose a simpler trusted-code product model: any UI Contribution
installed or explicitly configured by an App author or authorized operator may
execute as trusted application code, including a remote ESM URL. Lenso does not
mandate the two trust classes proposed by this review. The findings below remain
the evidence and implementation guide for an optional sandboxing Browser
Adapter or deployment policy; they are not Kernel or base Console requirements.

## Threat model

The design must remain safe when a remote page is malicious, becomes compromised after installation, changes without notice, or loads a different document after a redirect. Such a page must not be able to:

- read or modify the Console DOM;
- read Console storage, session tokens, credentials, or ActorAssertions;
- invoke arbitrary Capability operations;
- discover the global Capability Registry;
- navigate or replace the Console window;
- open privileged popups, start downloads, submit forms, or use powerful browser features unless explicitly allowed;
- turn broker parsing bugs into script execution in the Console realm; or
- retain authority after its frame navigates, reloads, or is removed.

The design does not claim to safely execute hostile JavaScript in the Console realm. Once code executes in that realm, browser primitives do not provide a meaningful per-module authority boundary.

## Mechanism comparison

| Mechanism | Console DOM and credentials | Isolation properties | Integrity and update behavior | Appropriate use |
|---|---|---|---|---|
| Same-origin dynamic ESM | Full Console-realm authority. JavaScript modules can access page globals such as `document`; same-origin requests include credentials by default. | None between the contribution and Console. CSP can decide whether the script loads, but once allowed it is trusted code. | Can be content-pinned when every executable resource is locked, but a mutable URL is a mutable authority grant. | Trusted, locally installed bundles only. |
| Cross-origin ESM | Still full Console-realm authority after loading. CORS governs the fetch, not the authority of the executed module. | No security compartment. Cross-origin module scripts require CORS, but execute as part of the importing Console document. | Entry-script SRI can pin the initial fetch; dependency integrity requires additional metadata and complete graph control. | Trusted, content-pinned code only; never an untrusted plugin boundary. |
| Web Component / Shadow DOM | Full authority of the JavaScript realm that defines it. It can use global browser APIs and reach surrounding page state. | Useful DOM and CSS encapsulation, not a hostile-code boundary. MDN explicitly warns that closed Shadow DOM is not a strong security mechanism. | Inherits the integrity and trust properties of the script that defines the component. | Composition and styling for trusted bundles. |
| Unsandboxed cross-origin iframe | Same-origin policy blocks direct parent DOM access, but the frame retains its own origin state and remains able to navigate, message, and use browser features unless separately restricted. | Better than ESM, but unnecessarily broad for arbitrary content. Cross-origin status alone is not a complete policy. | The remote document can change at any time. | Not the Lenso default. |
| Sandboxed iframe without `allow-same-origin` | Cannot pass same-origin checks against Console or the remote site's ordinary origin state. The document receives an opaque origin. | Strongest broadly available baseline for arbitrary executable pages. Start with `sandbox="allow-scripts"`; every additional token expands authority. | The remote document remains mutable and cannot be pinned by iframe SRI. Safety must not depend on its content staying benign. | Arbitrary external pages, through a narrow capability broker. |
| Locally installed trusted bundle | Full Console-realm authority, by design. | No runtime security boundary; trust comes from installation policy and immutable package resolution. Shadow DOM may still provide layout encapsulation. | Package version and lockfile can make the selected artifact reproducible; Console CSP can prohibit runtime remote script injection. | First-party and explicitly trusted third-party UI Contributions. |

JavaScript modules do not create an authority compartment. The module guide demonstrates that global values and `document` remain available inside a module, while the script element documentation states that cross-origin module loading uses CORS. CORS therefore answers whether bytes may be fetched; it does not make the fetched code less privileged once evaluated in the Console page. [MDN JavaScript modules](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules#other_differences_between_modules_and_classic_scripts), [MDN `script` element](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/script#module)

This distinction matters even when Console authentication uses `HttpOnly` cookies. `HttpOnly` prevents direct JavaScript reads, but the browser still attaches the cookie to eligible requests; `fetch` includes credentials on same-origin requests by default. Same-realm plugin code can therefore exercise any same-origin endpoint authorized by the user's session unless the server independently constrains that endpoint. [MDN `Set-Cookie`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie#httponly), [MDN Fetch credentials](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch#including_credentials)

Web Components solve a different problem. Shadow DOM can reduce accidental style and tree coupling, but it does not constrain the JavaScript that created the component. MDN explicitly describes closed Shadow DOM as an indication rather than a strong security mechanism. [MDN Shadow DOM](https://developer.mozilla.org/en-US/docs/Web/API/Web_components/Using_shadow_DOM#elementshadowroot_and_the_mode_option)

## Why an opaque-origin iframe is the remote baseline

An iframe with `sandbox` and without `allow-same-origin` is assigned a new opaque origin. Opaque origins fail same-origin checks against other resources and serialize as `null`. The HTML Standard also warns that combining `allow-scripts` and `allow-same-origin` for content that can become same-origin with its parent can let the child remove the sandbox and reload without its restrictions. [WHATWG iframe sandbox](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#attr-iframe-sandbox), [MDN origin glossary](https://developer.mozilla.org/en-US/docs/Glossary/Origin#opaque_origin)

The default remote frame should therefore begin with a deliberately small policy:

```html
<iframe
  sandbox="allow-scripts"
  referrerpolicy="no-referrer"
  allow="camera 'none'; microphone 'none'; geolocation 'none'; fullscreen 'none'"
></iframe>
```

The default must not include:

- `allow-same-origin`;
- `allow-forms`;
- `allow-popups` or `allow-popups-to-escape-sandbox`;
- `allow-top-navigation` or its variants;
- `allow-downloads`;
- `allow-modals`;
- `allow-storage-access-by-user-activation`; or
- powerful-feature grants in the iframe `allow` policy.

Any exception must be an explicit App policy for that contribution, not a field the remote page can request and self-approve. The sandbox token meanings and the same-origin escape warning are defined by the HTML Standard; Permissions Policy provides a separate way to restrict powerful features such as camera, microphone, and geolocation in embedded content. [WHATWG sandbox tokens](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#attr-iframe-sandbox), [MDN Permissions Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Permissions_Policy#embedded_frame_syntax)

`referrerpolicy="no-referrer"` prevents the initial iframe request from receiving a Console URL through the `Referer` header. Console must also keep authentication cookies host-only, `Secure`, `HttpOnly`, and normally `SameSite=Lax` or `Strict`; it must not use a broad `Domain` cookie that becomes available to arbitrary subdomains. The `__Host-Http-` prefix expresses the strongest browser-enforced host and HTTP-only restrictions where supported. [MDN iframe referrer policy](https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/referrerPolicy), [MDN cookie prefixes and scope](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Cookies#cookie_prefixes)

The experimental `credentialless` iframe attribute can provide useful additional isolation by loading third-party content in an ephemeral context without its ordinary cookies or local storage. It is not Baseline and must be treated only as progressive hardening, not as the portable security foundation. [MDN credentialless iframe](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/IFrame_credentialless)

## Capability broker design

The iframe must not receive a generic Console API, a bearer token, a Session cookie, an ActorAssertion, or a client that can resolve arbitrary Capability names. It receives a short-lived object-capability channel for only the operations granted to its contribution instance.

```text
App Composition
    resolves UI contribution requirements
                |
                v
Console Browser Adapter
    creates an operation-scoped broker session
                |
                v
dedicated MessagePort
                |
                v
sandboxed opaque-origin iframe
```

### Channel establishment

Because an opaque origin serializes as `null`, the normal recommendation to authenticate a sender through an exact `event.origin` cannot distinguish opaque-origin frames. This is an intentional trade-off: Lenso authenticates the configured contribution instance, not an HTTP origin exposed to the guest page. During the one-time bootstrap, Console must:

1. hold the exact `iframe.contentWindow` reference;
2. accept the bootstrap message only when `event.source === iframe.contentWindow`;
3. verify a per-load random challenge and a strict protocol-versioned message shape;
4. create a fresh `MessageChannel` and transfer only one port to that window;
5. stop using the global `message` event for normal calls after the port handoff; and
6. close the port and revoke the broker session on navigation, reload, frame removal, timeout, or protocol violation.

The handoff to an opaque origin may require `targetOrigin="*"`, because there is no stable serialized target origin to name. It must still target the exact frame window, and the bootstrap message must contain no credential or reusable authority. The security boundary is the freshly transferred port plus the broker's server-side grant, not secrecy of the bootstrap message. This is an inference from opaque-origin serialization and the `postMessage` targeting model. MDN requires validation of both sender identity and message syntax, while the HTML Standard explicitly describes `MessagePort` as a basis for object-capability APIs. [MDN `postMessage` security](https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage#security_concerns), [WHATWG ports as object capabilities](https://html.spec.whatwg.org/multipage/web-messaging.html#ports-as-the-basis-of-an-object-capability-model-on-the-web)

### Broker grant

The grant should be derived from App Composition and contain at least:

```text
contribution instance key
allowed Capability contract versions
allowed operation IDs
interaction kinds
user/Actor audience policy
expiry
maximum message and response sizes
concurrency and rate limits
```

The remote page receives no raw ActorAssertion. For each call, the Browser Adapter establishes or propagates the current user context, checks that the operation is in the broker grant, and lets the target business Module perform final authorization. A remote page must never inherit an ambient Console-administrator identity merely because it is rendered inside Console.

The broker must also:

- reject unknown fields and unknown operation IDs;
- validate request and response values against generated portable Capability schemas;
- use bounded queues and explicit `ResourceExhausted`, deadline, and cancellation behavior;
- rate-limit each contribution instance;
- cap strings, arrays, binary payloads, streams, and outstanding requests;
- avoid merging guest objects into privileged objects or writing guest strings with `innerHTML`;
- return only the minimum result shape needed by the granted operation;
- require explicit user confirmation outside the iframe for selected destructive operations; and
- emit security/audit events from the broker and target Module rather than treating best-effort Runtime Diagnostics as an audit log.

The remote page may exfiltrate every value the broker returns to it. The broker must therefore treat response disclosure as an authority grant, not merely control which methods can be called.

## CSP and origin policy

Console CSP and iframe sandboxing have different jobs:

- `script-src` protects the Console realm from unapproved JavaScript.
- `frame-src` controls which locations Console may embed.
- the iframe `sandbox` attribute constrains the embedded browsing context.
- the embedded server's `frame-ancestors` policy decides whether it permits Console to embed it.

Allowing a host in `frame-src` does not make that host's JavaScript safe or apply Console's `script-src` to the child's own document. Conversely, an external page can refuse embedding with `frame-ancestors` or `X-Frame-Options`, so Lenso cannot truthfully promise that every arbitrary URL will render. [W3C CSP `frame-src`](https://www.w3.org/TR/CSP3/#directive-frame-src), [MDN `frame-ancestors`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors)

Console should use a strict CSP along these lines, adjusted to its actual asset pipeline:

```text
default-src 'none';
script-src 'self' <nonces-or-hashes>;
style-src 'self' <required-style-policy>;
connect-src 'self';
img-src 'self' data:;
object-src 'none';
base-uri 'none';
frame-src <origins-declared-by-app-composition>;
frame-ancestors 'none';
```

If runtime registration truly permits any HTTPS host, `frame-src https:` is possible but materially weaker than a Composition-derived origin list. Even then, the remote URL must remain forbidden as a `script-src` source. URL registration should accept HTTPS only and reject Console, authentication, loopback, local-network, `file:`, `data:`, `blob:`, and `javascript:` targets unless a separately reviewed use case requires one. Redirect and navigation behavior must be treated as part of the contribution's remote-content risk.

Cross-origin isolation headers are not a substitute for this design. COOP and COEP solve browsing-context-group and cross-origin resource-isolation problems; COEP can also prevent non-cooperating third-party pages from loading. They do not turn cross-origin ESM into an unprivileged Console plugin. [MDN cross-origin isolation requirements](https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage#secure_shared_memory_messaging), [MDN credentialless iframe and COEP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/IFrame_credentialless#the_problem)

## Integrity and TOCTOU

Subresource Integrity currently provides browser verification for `script` and selected `link` resources, not for an iframe's HTML document. The HTML Standard further specifies that a script element's `integrity` metadata applies to the initial external-script fetch; module dependency integrity requires separately supplied import-map integrity metadata. [MDN Subresource Integrity](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Subresource_Integrity#using_subresource_integrity), [WHATWG script integrity](https://html.spec.whatwg.org/multipage/scripting.html#attr-script-integrity), [WHATWG import-map integrity](https://html.spec.whatwg.org/multipage/webappapis.html#import-map-integrity-metadata)

Consequences:

- Cross-origin ESM is only reproducible when the entry and complete dependency graph are pinned and CORS-compatible.
- A parent cannot attach SRI metadata to an iframe and thereby pin the remote HTML page and everything it loads.
- Server-side prefetch-and-hash followed by a browser request to the same URL is a time-of-check/time-of-use gap, because the browser executes a different fetch result.
- Fetching remote HTML and re-serving it from the Console origin would collapse the origin boundary and is unacceptable.
- A mutable external page must stay within the same minimal broker grant even after its content changes.
- Code that needs sensitive authority or review-based trust must be packaged, locked, and installed as a Trusted UI Contribution instead of loaded from a mutable URL.

Lenso does not need a custom Module Release verification system to achieve the trusted mode. The App's ordinary package manager and lockfile can select immutable assets; the Console build or asset server can then serve only that installed material under a strict CSP. Remote content remains a different, explicitly untrusted product feature.

## Recommended Lenso model

The UI Contribution Capability should describe both presentation metadata and a trust-specific execution kind:

```yaml
ui:
  kind: trusted-bundle | sandboxed-external
  route: /billing
  navigation:
    label: Billing

  # trusted-bundle
  entrypoint: ./dist/billing-ui.js

  # sandboxed-external
  url: https://ui.example.com/lenso

  requires:
    - capability: billing.invoice@1
      operations: [list, get]
```

This is still one ordinary UI Contribution Capability, but its Browser Adapter selects one of two execution policies:

### Trusted UI Contribution

- resolved from an App-locked local package;
- may use ESM and Web Components in the Console realm;
- receives generated clients for declared portable requirements as an ergonomics boundary;
- is nevertheless trusted with the Console realm, because browser JavaScript cannot enforce least authority among same-realm modules; and
- cannot introduce runtime remote scripts under Console CSP.

### Sandboxed External Page

- loads only an App-approved HTTPS URL;
- runs in an opaque-origin iframe with the minimal sandbox;
- receives a fresh, bounded, operation-scoped `MessagePort` broker session;
- never receives Console credentials, tokens, raw ActorAssertions, the global Registry, or general-purpose same-origin fetch authority;
- is visibly labeled as external content so it cannot silently imitate trusted Console chrome;
- loses its channel on navigation or reload;
- is not promised content integrity, offline availability, embed compatibility, or stable behavior; and
- cannot be promoted to trusted UI merely because it is cross-origin or uses Web Components.

## Decision guidance for Q76

The original choice, “any URL may be injected into Console,” is unsafe if “injected” includes remote ESM, Web Components, or any other same-realm execution. A precise safe version is:

> Lenso may display an arbitrary App-approved HTTPS URL as a sandboxed External Page. Remote JavaScript never executes in the Console Shell realm. The page receives only explicitly declared, Composition-resolved, short-lived Capability operations through a bounded broker. UI requiring trusted Console integration must ship as a locally installed and locked Trusted UI Contribution.

This preserves custom-page flexibility without making every remote page a Console administrator. It also keeps the architectural boundary honest: CSP, CORS, Shadow DOM, and package metadata are useful controls, but only an isolated browsing context plus an explicit broker can keep arbitrary remote code outside the Console authority domain.
