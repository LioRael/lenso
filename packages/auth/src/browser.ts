/** Public UI descriptions contain data, never credentials or executable code. */
export type LoginMethod =
  | {
      readonly id: string;
      readonly kind: "password";
      readonly label: string;
      readonly action: string;
    }
  | {
      readonly id: string;
      readonly kind: "redirect";
      readonly label: string;
      readonly action: string;
    }
  | {
      readonly id: string;
      readonly kind: "custom";
      readonly renderer: string;
      readonly label: string;
      readonly action: string;
    };

export interface AuthDiscovery {
  readonly methods: readonly LoginMethod[];
  readonly session?: { readonly logout?: string; readonly renew?: string };
  readonly csrf?: { readonly cookie_name: string; readonly header_name: "x-csrf-token" };
}
