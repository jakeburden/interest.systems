import { address, generateKeyPairSigner } from "gill";
import { initClient, sendIxs } from "./client.js";

async function main() {
  const { rpc } = initClient("localnet");
  // placeholder – real example will construct accounts and use `sendIxs`
  console.log("Gill client ready; see surfpool runbooks for end-to-end examples.");
}
main().catch(console.error);


