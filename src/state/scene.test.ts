import { describe, expect, it, vi } from "vitest";
import { createDebouncedSaver, emptyScene, parseScene, serializeScene } from "./scene";

describe("parseScene", () => {
  it("返回空 scene 当 JSON 损坏时（不崩溃）", () => {
    expect(parseScene("{ not valid json")).toEqual(emptyScene());
  });

  it("返回空 scene 当顶层不是对象时", () => {
    expect(parseScene("42")).toEqual(emptyScene());
    expect(parseScene('"hello"')).toEqual(emptyScene());
  });

  it("返回空 scene 当缺少 elements 数组时", () => {
    expect(parseScene('{"appState":{}}')).toEqual(emptyScene());
  });

  it("解析合法 scene 并保留 elements 与 appState", () => {
    const json = JSON.stringify({
      version: 1,
      elements: [{ id: "a", type: "rectangle", x: 1, y: 2 }],
      appState: { viewBackgroundColor: "transparent" },
    });
    const scene = parseScene(json);
    expect(scene.elements).toHaveLength(1);
    expect(scene.appState.viewBackgroundColor).toBe("transparent");
  });

  it("round-trip：serialize -> parse 保持数据一致", () => {
    const scene = parseScene(
      JSON.stringify({
        version: 1,
        elements: [{ id: "x", type: "freedraw", points: [] }],
        appState: { viewBackgroundColor: "transparent" },
      }),
    );
    const reparsed = parseScene(serializeScene(scene.elements, scene.appState));
    expect(reparsed).toEqual(scene);
  });
});

describe("createDebouncedSaver", () => {
  it("连续 schedule 只触发一次保存", async () => {
    vi.useFakeTimers();
    const save = vi.fn().mockResolvedValue(undefined);
    const saver = createDebouncedSaver(save, 500);

    saver.schedule(emptyScene());
    saver.schedule(emptyScene());
    saver.schedule(emptyScene());
    expect(save).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(500);
    expect(save).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it("flush 立即保存尚未落盘的内容", async () => {
    vi.useFakeTimers();
    const save = vi.fn().mockResolvedValue(undefined);
    const saver = createDebouncedSaver(save, 500);

    saver.schedule(emptyScene());
    const flushPromise = saver.flush();
    await flushPromise;

    expect(save).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it("flush 等待正在保存的内容和随后排队的更新", async () => {
    const releases: Array<() => void> = [];
    let secondStarted!: () => void;
    const secondSaveStarted = new Promise<void>((resolve) => {
      secondStarted = resolve;
    });
    const save = vi.fn(() => {
      if (releases.length === 1) secondStarted();
      return new Promise<void>((resolve) => releases.push(resolve));
    });
    const saver = createDebouncedSaver(save);

    saver.schedule(emptyScene());
    const firstFlush = saver.flush();
    saver.schedule(emptyScene());
    let flushed = false;
    const finalFlush = saver.flush().then(() => {
      flushed = true;
    });

    expect(save).toHaveBeenCalledTimes(1);
    releases[0]();
    await secondSaveStarted;
    expect(save).toHaveBeenCalledTimes(2);
    expect(flushed).toBe(false);

    releases[1]();
    await firstFlush;
    await finalFlush;
    expect(flushed).toBe(true);
  });

  it("cancel 清空待保存内容", async () => {
    vi.useFakeTimers();
    const save = vi.fn().mockResolvedValue(undefined);
    const saver = createDebouncedSaver(save, 500);

    saver.schedule(emptyScene());
    saver.cancel();
    await vi.advanceTimersByTimeAsync(500);

    expect(save).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
