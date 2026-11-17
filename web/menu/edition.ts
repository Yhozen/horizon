// What differs between the two sites that share this menu: horizon.rccpr.tech (the
// private preview, Minecraft's own art) and open.rccpr.tech (Horizon Open Creative,
// docs/open-creative.md). Everything else in the menu is the same.
import { Schema } from "effect";

export const EditionId = Schema.Literals(["horizon", "open"]);
export type EditionId = typeof EditionId.Type;

export const WorldId = Schema.Literals(["survival", "shared", "solo"]);
export type WorldId = typeof WorldId.Type;

export interface World {
  readonly id: WorldId;
  readonly title: string;
  /** One line on the world's card. */
  readonly card: string;
  /** The longer description in "How to play". */
  readonly about: string;
  /** Background art: a key of the bundled pictures (scripts/menu.mjs). */
  readonly art: string;
  /** Shown under the cards while this world is chosen. */
  readonly caveat?: string;
}

export interface Edition {
  readonly id: EditionId;
  readonly name: string;
  readonly tagline: string;
  readonly title: string;
  readonly worlds: readonly World[];
  /** The game's single player world: its name, and whether it is creative. */
  readonly solo: { readonly world: string; readonly creative: boolean };
  /** The release's edition field, when the site only runs one edition. */
  readonly release?: string;
  readonly credits: boolean;
}

const survival = (art: string, caveat?: string): World => ({
  id: "survival", title: "Survival", card: "With Java Edition players",
  about: "The long-running world on survival.minecraft.rccpr.tech. Java Edition players join it too.",
  art, ...(caveat && { caveat }),
});
const shared = (art: string): World => ({
  id: "shared", title: "Horizon world", card: "Horizon and Open players",
  about: "Horizon's own server, shared by horizon.rccpr.tech and open.rccpr.tech. It starts fresh when the server restarts.",
  art,
});

export const EDITIONS: Readonly<Record<EditionId, Edition>> = {
  horizon: {
    id: "horizon", name: "Horizon", title: "Horizon",
    tagline: "Minecraft: Java Edition 26.3, rebuilt in Rust. Runs in your browser.",
    worlds: [survival("village"), shared("night"), {
      id: "solo", title: "Single player", card: "Just you, in creative",
      about: "A new creative world on this device. Nothing is shared.", art: "sunset" }],
    solo: { world: "Web World", creative: true },
    credits: false,
  },
  open: {
    id: "open", name: "Horizon Open", title: "Horizon Open Creative",
    tagline: "An open block world built from freely licensed art. No Minecraft files needed.",
    worlds: [
      survival("meadow", "Seen through Open, some blocks show as their nearest match, and creatures aren't visible yet."),
      shared("meadow"),
      { id: "solo", title: "Single player", card: "Just you, in creative",
        about: "Your own creative world with every block. It isn't saved: reloading the page starts a new one.",
        art: "meadow" }],
    solo: { world: "Open World", creative: true },
    release: "open-creative",
    credits: true,
  },
};

/** Minecraft's rule for player names, which every server here accepts. */
export const NAME = /^[A-Za-z0-9_]{3,16}$/;
