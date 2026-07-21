// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Activity, Tag } from "../../lib/types";

const { tags } = vi.hoisted(() => ({
  tags: [
    { id: 1, name: "tag-a" },
    { id: 2, name: "tag-b" },
    { id: 3, name: "tag-c" },
    { id: 4, name: "tag-d" },
    { id: 5, name: "tag-e" },
  ] as Tag[],
}));

const addToast = vi.fn();

vi.mock("../../lib/tauri", () => ({
  api: {
    getTags: vi.fn().mockResolvedValue(tags),
    createTag: vi.fn(),
    updateActivity: vi.fn().mockResolvedValue(undefined),
    setActivityTags: vi.fn().mockResolvedValue(undefined),
    updateActivityLocation: vi.fn().mockResolvedValue({ geocoded: true, location_name: "" }),
    deleteActivity: vi.fn().mockResolvedValue(undefined),
  },
}));
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { EditActivityModal } from "./EditActivityModal";

afterEach(() => {
  cleanup();
  addToast.mockClear();
});

const activity = { id: "act-1", title: "Run", notes: null, sport_type: "run", location_name: null } as Activity;

function renderModal() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <EditActivityModal
        activity={activity}
        currentTags={[]}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />
    </QueryClientProvider>
  );
}

describe("EditActivityModal tag cap", () => {
  it("blocks selecting a 4th tag", async () => {
    const { getByRole, getByPlaceholderText } = renderModal();

    await waitFor(() => getByRole("button", { name: "tag-a" }));

    // Select three tags.
    fireEvent.click(getByRole("button", { name: "tag-a" }));
    fireEvent.click(getByRole("button", { name: "tag-b" }));
    fireEvent.click(getByRole("button", { name: "tag-c" }));

    // The remaining (unselected) tags are now disabled, and so is the new-tag
    // input — you can't add a 4th.
    await waitFor(() => {
      expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(true);
      expect((getByRole("button", { name: "tag-e" }) as HTMLButtonElement).disabled).toBe(true);
    });
    expect((getByPlaceholderText("Up to 3 tags selected") as HTMLInputElement).disabled).toBe(true);

    // Clicking a disabled tag does nothing; deselecting one frees a slot again.
    fireEvent.click(getByRole("button", { name: "tag-d" }));
    expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(getByRole("button", { name: "tag-a" })); // deselect a-tag
    await waitFor(() => {
      expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(false);
    });
  });
});
