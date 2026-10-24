import { writable } from "svelte/store";

// Whether the explanatory notes are shown: a panel beside each page and a
// short line under each setting. Off on every launch; the user turns them on
// with the Notes button in the header (or Ctrl+I).
export const showNotes = writable<boolean>(false);
