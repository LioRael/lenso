import type { WorkerApp } from "../src/index";

const synchronous: NonNullable<WorkerApp["bind"]> = () => {};
// @ts-expect-error Binding does not accept an async lifecycle callback.
const asynchronous: NonNullable<WorkerApp["bind"]> = async () => {};
// @ts-expect-error Binding cannot register a returned cleanup function.
const cleanup: NonNullable<WorkerApp["bind"]> = () => () => {};

void synchronous;
void asynchronous;
void cleanup;
