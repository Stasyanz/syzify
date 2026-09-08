// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { PluginsCard } from "./PluginsCard";

afterEach(cleanup);

describe("PluginsCard", () => {
  it("leads to the plugin registry", async () => {
    render(
      <MemoryRouter initialEntries={["/settings"]}>
        <Routes>
          <Route path="/settings" element={<PluginsCard />} />
          <Route path="/plugins" element={<div>REGISTRY</div>} />
        </Routes>
      </MemoryRouter>,
    );
    expect(screen.getByText("Plugins & extensions")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Manage/ }));
    await waitFor(() => expect(screen.getByText("REGISTRY")).toBeTruthy());
  });
});
