import { createClient } from "#lenso/client";
const api = createClient(process.env.LENSO_URL ?? "http://127.0.0.1:3000/rpc");
const name = process.argv[2] ?? "Ada";
console.log(JSON.stringify(await api.greet({ name }), null, 2));
console.log(JSON.stringify(await api.status(), null, 2));
