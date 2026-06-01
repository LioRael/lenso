import { useCallback, useEffect, useState } from "react";

export function usePersistedLayout<T>(key: string, defaultValue: T) {
  const [value, setValue] = useState<T>(() => {
    if (typeof window === "undefined") {
      return defaultValue;
    }

    const stored = window.localStorage.getItem(key);
    if (!stored) {
      return defaultValue;
    }

    try {
      const parsed = JSON.parse(stored) as T;
      if (isObjectValue(defaultValue) && !isObjectValue(parsed)) {
        return defaultValue;
      }
      return parsed;
    } catch {
      return defaultValue;
    }
  });

  useEffect(() => {
    window.localStorage.setItem(key, JSON.stringify(value));
  }, [key, value]);

  const reset = useCallback(() => setValue(defaultValue), [defaultValue]);

  return [value, setValue, reset] as const;
}

function isObjectValue(value: unknown) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
