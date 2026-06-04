import { test as base, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

type Fixtures = {
  axe: (selector?: string) => Promise<void>;
};

export const test = base.extend<Fixtures>({
  axe: async ({ page }, registerFixture) => {
    await registerFixture(async (selector?: string) => {
      const builder = new AxeBuilder({ page }).withTags([
        "wcag2a",
        "wcag2aa",
        "wcag21a",
        "wcag21aa",
      ]);
      if (selector) builder.include(selector);
      const results = await builder.analyze();
      expect(results.violations, JSON.stringify(results.violations, null, 2)).toEqual([]);
    });
  },
});

export { expect };
