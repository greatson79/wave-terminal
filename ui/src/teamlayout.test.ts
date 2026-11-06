import { expect, test } from "bun:test";
import { loadTeamLayout, saveTeamLayout, TEAM_LAYOUT_KEY, LEGACY_LAYOUT_KEY, LAYOUT_BACKUP_KEY, LAYOUT_VERSION_KEY } from "./teamlayout";

function storage(initial: Record<string, string> = {}, denied?: string) {
  const values = new Map(Object.entries(initial));
  const writes: string[] = [];
  return { values, writes, getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      writes.push(key);
      if (key === denied) throw new Error("storage denied");
      values.set(key, value);
    } };
}

test("reading an existing learner layout does not write or alter its original bytes", () => {
  const raw = ' { "workspaces": [], "groups": [] }\n';
  const s = storage({ [LEGACY_LAYOUT_KEY]: raw });
  expect(loadTeamLayout(s)).toEqual({ workspaces: [], groups: [] });
  expect(s.writes).toEqual([]);
  expect(s.getItem(LEGACY_LAYOUT_KEY)).toBe(raw);
  expect(loadTeamLayout(storage())).toBeNull();
});

test("explicit save keeps one byte-exact legacy backup and marks version after payload", () => {
  const raw = ' { "workspaces": [] }\n';
  const s = storage({ [LEGACY_LAYOUT_KEY]: raw });
  const layout = { workspaces: [{ id: 1, teamId: "a", name: "기획팀", tree: null }] };
  saveTeamLayout(s, layout);
  expect(s.writes).toEqual([LAYOUT_BACKUP_KEY, TEAM_LAYOUT_KEY, LAYOUT_VERSION_KEY]);
  expect(s.getItem(LEGACY_LAYOUT_KEY)).toBe(raw);
  expect(s.getItem(LAYOUT_BACKUP_KEY)).toBe(raw);
  expect(s.getItem(LAYOUT_VERSION_KEY)).toBe("3");
  expect(loadTeamLayout(s)).toEqual(layout);
  s.setItem(LEGACY_LAYOUT_KEY, "changed elsewhere");
  saveTeamLayout(s, { workspaces: [] });
  expect(s.getItem(LAYOUT_BACKUP_KEY)).toBe(raw);
  expect(loadTeamLayout(s)).toEqual({ workspaces: [] });
});

test("denied new-layout write retains original and backup without a version mark", () => {
  const raw = '{"workspaces":[]}';
  const s = storage({ [LEGACY_LAYOUT_KEY]: raw }, TEAM_LAYOUT_KEY);
  expect(() => saveTeamLayout(s, { workspaces: [] })).toThrow("storage denied");
  expect(s.getItem(LEGACY_LAYOUT_KEY)).toBe(raw);
  expect(s.getItem(LAYOUT_BACKUP_KEY)).toBe(raw);
  expect(s.getItem(LAYOUT_VERSION_KEY)).toBeNull();
  expect(s.writes).toEqual([LAYOUT_BACKUP_KEY, TEAM_LAYOUT_KEY]);
});

test("denied backup write prevents migration writes; corrupt new layout is not silently replaced", () => {
  const s = storage({ [LEGACY_LAYOUT_KEY]: "{}" }, LAYOUT_BACKUP_KEY);
  expect(() => saveTeamLayout(s, {})).toThrow("storage denied");
  expect(s.getItem(TEAM_LAYOUT_KEY)).toBeNull();
  expect(s.getItem(LAYOUT_VERSION_KEY)).toBeNull();
  expect(() => loadTeamLayout(storage({ [TEAM_LAYOUT_KEY]: "broken", [LEGACY_LAYOUT_KEY]: "{}" }))).toThrow();
});
