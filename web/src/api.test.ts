import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchAudioFile, setToken } from "./api";

describe("fetchAudioFile", () => {
  afterEach(() => {
    setToken("");
    vi.restoreAllMocks();
  });

  it("uses bearer authentication without putting the token in the URL", async () => {
    setToken("test token&value");
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(new Blob(["audio"]), { status: 200 }));

    await fetchAudioFile(42);

    expect(fetchMock).toHaveBeenCalledWith("/api/audio/42/file", {
      headers: { Authorization: "Bearer test token&value" },
    });
  });
});
