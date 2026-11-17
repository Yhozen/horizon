// Boots the menu on a page whose <html data-edition> names its site ("horizon" or "open").
import { Runtime } from "foldkit";
import { Flags, Model, init, readFlags, subscriptions, update, view } from "./main.ts";

const edition = document.documentElement.dataset.edition === "open" ? "open" : "horizon";
Runtime.run(Runtime.makeApplication({
  Model, Flags, init, update, view, subscriptions,
  container: document.getElementById("menu"),
}), { flags: readFlags(edition) });

// Right click places blocks: the browser's context menu must not open over the game
// (it would also end the pointer lock).
document.getElementById("horizon")?.addEventListener("contextmenu", (event) => event.preventDefault());
