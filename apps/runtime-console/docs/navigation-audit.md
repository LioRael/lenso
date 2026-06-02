# Runtime Console Navigation Audit

This audit treats `/runtime/stories` as the primary runtime causality workbench. Pages remain first-class only when they answer a distinct operational question, require a different mental model, and are actionable in mock mode.

## Decisions

| Page         | Unique user question                                   | Already answered by Stories?                                                                | Sidebar?          | Destination                                |
| ------------ | ------------------------------------------------------ | ------------------------------------------------------------------------------------------- | ----------------- | ------------------------------------------ |
| Stories      | What happened across one correlated runtime execution? | No. This is the canonical causality model.                                                  | Yes               | Primary workbench                          |
| Events       | Which outbox events exist by status?                   | Mostly. Event rows depend on `correlation_id` and are best inspected through story context. | No                | Merge into Stories as future filtered view |
| Functions    | Which function runs exist by status/attempt?           | Mostly. Function runs are story-linked runtime items.                                       | No                | Merge into Stories as future filtered view |
| Timeline     | What happened for a correlation ID?                    | Yes. This is the story workbench's core job.                                                | No                | Redirect to Stories                        |
| Queues       | Where is runtime pressure accumulating?                | No. Queue pressure is aggregate backlog state, not single-story causality.                  | Yes               | Primary page                               |
| Flows        | What configured workflow graph exists?                 | Not supported by current mock/backend model.                                                | No                | Future/deferred                            |
| Agents       | What agent/tool activity exists?                       | Not supported by current mock/backend model.                                                | No                | Future/deferred                            |
| Dead Letters | What failed work needs operator action?                | Partially, but remediation is a distinct inbox mental model.                                | Yes               | Primary page                               |
| Overview     | Is the runtime healthy right now?                      | No. It aggregates posture across queues, stories, and failures.                             | Yes               | Primary page                               |
| Settings     | What runtime/environment controls are available?       | No, but current prototype has no real settings backend.                                     | Compact secondary | Deferred placeholder                       |

## Recommended Sidebar

Primary:

- Stories
- Dead Letters
- Queues
- Overview

Secondary:

- Settings

Deferred / hidden from primary:

- Events
- Functions
- Timeline
- Flows
- Agents

## Rationale

- `Timeline` is redundant with Stories because both answer correlation-level causality. It should become correlation search/result behavior inside Stories.
- `Events` and `Functions` are useful record types, but as first-class pages they duplicate story-linked inspection. They should return as filtered Stories views once the backend supports event/function scoped queries.
- `Dead Letters` stays because it is actionable: operators need a failure inbox with retry/error context.
- `Queues` stays because queue pressure is an aggregate operational model, not a single-story model.
- `Overview` stays because it answers runtime posture, not individual execution causality.
- `Flows` and `Agents` should not be prominent until real runtime support exists.

## Implementation Notes

- The sidebar should expose only current, actionable pages.
- Legacy routes for merged pages should redirect to `/runtime/stories` rather than becoming broken links.
- Command palette should prefer the simplified IA.
- Search can still find event/function records as runtime objects, but those records should not justify separate primary navigation.
