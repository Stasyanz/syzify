import { describe, it, expect, afterEach } from "vitest";
import { errorCode, errorText } from "./errors";
import { useLanguageStore } from "./i18n";
import { commandError } from "../test/commandError";

describe("errorText", () => {
  afterEach(() => useLanguageStore.getState().setPref("system"));

  it("reads a bare string as it is", () => {
    expect(errorText("Gear not found: g-1")).toBe("Gear not found: g-1");
  });

  it("reads an Error's message, or its name when the message is empty", () => {
    expect(errorText(new Error("boom"))).toBe("boom");
    expect(errorText(new TypeError("typed"))).toBe("typed");
    expect(errorText(new RangeError())).toBe("RangeError");
  });

  it("reads the message off a structured rejection, and stringifies the rest", () => {
    expect(errorText({ message: "quota exceeded" })).toBe("quota exceeded");
    expect(errorText({ message: "" })).toBe("Unknown error");
    expect(errorText({ code: 7 })).toBe("Unknown error");
    expect(errorText(commandError("vault.fromTheFuture", ""))).toBe("Unknown error");
    expect(errorText({ other: 1 })).toBe("[object Object]");
    expect(errorText(42)).toBe("42");
  });

  it("never says undefined", () => {
    expect(errorText(undefined)).toBe("Unknown error");
    expect(errorText(null)).toBe("Unknown error");
    expect(errorText(42)).toBe("42");
  });

  it("says a coded command error in English as Rust wrote it", () => {
    const e = commandError("gear.notFound", "Gear not found: g-1", { id: "g-1" });
    expect(errorText(e)).toBe("Gear not found: g-1");
    expect(errorText(commandError("vault.wrongPassword", "Wrong password"))).toBe("Wrong password");
  });

  it("says a coded command error in the interface language, its params filled (#236)", () => {
    useLanguageStore.getState().setPref("ru");
    expect(errorText(commandError("vault.wrongPassword", "Wrong password"))).toBe("Неверный пароль");
    expect(errorText(commandError("gear.notFound", "Gear not found: g-1", { id: "g-1" }))).toBe(
      "Снаряжение не найдено: g-1",
    );
    // A limit agrees with its number: "длиннее 61 символа", "длиннее 60 символов".
    const tooLong = (count: number) =>
      errorText(commandError("gear.nameTooLong", `Name is longer than ${count} characters`, { count }));
    expect(tooLong(61)).toBe("Название длиннее 61 символа");
    expect(tooLong(60)).toBe("Название длиннее 60 символов");
    expect(tooLong(2)).toBe("Название длиннее 2 символов");
    // A number is written the language's way.
    useLanguageStore.getState().setPref("it");
    const ftp = commandError("activity.invalidFtp", "invalid FTP: 12345.5", { ftp: 12345.5 });
    expect(errorText(ftp)).toBe("FTP non valido: 12.345,5");
  });

  it("says the message for an unknown code, no code, or params it cannot fill", () => {
    useLanguageStore.getState().setPref("ru");
    expect(errorText(commandError("vault.fromTheFuture", "Something new"))).toBe("Something new");
    expect(errorText(commandError(null, "disk I/O error"))).toBe("disk I/O error");
    // An own key only: "error.constructor" is no message.
    expect(errorText(commandError("constructor", "Odd"))).toBe("Odd");
    // A placeholder without a value, or with an object for one.
    expect(errorText(commandError("gear.notFound", "Gear not found: g-1"))).toBe("Gear not found: g-1");
    expect(errorText({ code: "gear.notFound", params: { id: { nested: 1 } }, message: "Gear not found: ?" })).toBe(
      "Gear not found: ?",
    );
    expect(errorText({ code: "gear.notFound", params: null, message: "Gear not found: g-2" })).toBe(
      "Gear not found: g-2",
    );
    // A bare string stays as it is in any language: a plugin's words.
    expect(errorText("Gear not found: g-1")).toBe("Gear not found: g-1");
  });

  it("reads a command error's code, and none off anything else", () => {
    expect(errorCode(commandError("vault.wrongPassword", "Wrong password"))).toBe("vault.wrongPassword");
    expect(errorCode(commandError(null, "disk full"))).toBeNull();
    expect(errorCode({ code: 7 })).toBeNull();
    expect(errorCode("vault.wrongPassword")).toBeNull();
    expect(errorCode(null)).toBeNull();
  });

  it("says its own fallback in the interface language (#235)", () => {
    useLanguageStore.getState().setPref("ru");
    expect(errorText(undefined)).toBe("Неизвестная ошибка");
  });
});
