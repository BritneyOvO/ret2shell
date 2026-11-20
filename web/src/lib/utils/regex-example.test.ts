import { describe, expect, it } from "vitest";
import { generateRegexExample } from "./regex-example";

describe("generateRegexExample", () => {
  it("produces valid examples for negated classes", () => {
    const pattern = "^[^A-Z]{2}$";
    expect(generateRegexExample(pattern)).toMatch(new RegExp(pattern));
  });

  it("keeps examples for common Unicode and quantified patterns", () => {
    expect(generateRegexExample("\\A\\p{Han}{2}-[0-9]{3}\\z")).toBe("张张-000");
  });

  it("omits examples that cannot be generated within the team name limit", () => {
    expect(generateRegexExample("a{10000000}")).toBeNull();
    expect(generateRegexExample("[Z-A]")).toBeNull();
  });
});
