import { useBrowserUrlPopState } from "../hooks/use-browser-url-state";
import { readOperationsParam } from "./operations-url-model";

type OperationsUrlParamSetter<T> = {
  bivarianceHack(value: T): void;
}["bivarianceHack"];

type OperationsUrlParamBinding<T> = {
  name: string;
  parse?: (value: string) => T;
  setValue: OperationsUrlParamSetter<T>;
};

export function readOperationsParamValue<T = string>(
  name: string,
  parse?: (value: string) => T
) {
  const value = readOperationsParam(name);
  return parse ? parse(value) : (value as T);
}

export function useOperationsUrlPopState(
  bindings: Array<OperationsUrlParamBinding<unknown>>
) {
  useBrowserUrlPopState((search) => {
    for (const binding of bindings) {
      const value = search.get(binding.name) ?? "";
      binding.setValue(binding.parse ? binding.parse(value) : value);
    }
  });
}
