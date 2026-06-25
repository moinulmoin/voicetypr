import { describe, it, expect, vi, afterEach } from "vitest";
import { logger, createLogger, setLogLevel } from "./logger";

describe("logger", () => {
  afterEach(() => {
    setLogLevel("trace"); // restore permissive default for sibling tests
    vi.restoreAllMocks();
  });

  it("routes each level to the matching console method with original args", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    logger.warn("w", { a: 1 });
    logger.error("e");

    expect(warnSpy).toHaveBeenCalledWith("w", { a: 1 });
    expect(errorSpy).toHaveBeenCalledWith("e");
  });

  it("suppresses calls below the configured threshold before any sink", () => {
    const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    setLogLevel("error");
    logger.debug("dropped");
    logger.info("dropped");
    logger.error("kept");

    expect(debugSpy).not.toHaveBeenCalled();
    expect(errorSpy).toHaveBeenCalledWith("kept");
  });

  it("does not prefix the scope onto console args in tests", () => {
    const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {});

    createLogger("models").info("status", 3);

    expect(infoSpy).toHaveBeenCalledWith("status", 3);
  });
});
