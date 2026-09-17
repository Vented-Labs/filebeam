import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { expect, test } from "@playwright/test";

const run = promisify(execFile);
const native = process.env.NATIVE_ACCEPTANCE_BINARY!;

async function nativeCommand(args: string[]): Promise<string> {
  const { stdout } = await run(native, args, {
    env: process.env,
    timeout: 300_000,
  });
  return stdout.trim();
}

test("native hosted password note decrypts in browser", async ({ page }) => {
  const link = await nativeCommand([
    "create-note",
    "native to browser note",
    "browser-password",
    "burn",
  ]);
  await page.goto(link);
  await page.locator("#transfer-password").fill("browser-password");
  await page.getByRole("button", { name: "Unlock" }).click();
  await page.getByRole("button", { name: "Decrypt note" }).click();
  await expect(page.getByText("native to browser note")).toBeVisible();
  await expect(
    nativeCommand(["open-note", link, "browser-password"]),
  ).rejects.toThrow();
});

test("browser hosted password burn note decrypts through native services", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("tab", { name: "Notes" }).click();
  await page.locator(".cm-content").click();
  await page.locator(".cm-content").pressSequentially("browser to native note");
  await page.getByTestId("prism-password-trigger").click();
  const password = page
    .getByTestId("prism-password-popover")
    .locator("#transfer-password");
  await password.fill("native-password");
  await page
    .getByTestId("prism-password-popover")
    .getByRole("button", { name: "Done" })
    .click();
  await page.getByLabel("Burn on read").click();
  await page
    .getByRole("button", { name: /Send encrypted|Encrypt and share/ })
    .click();
  const link = await page.locator("#share-link").inputValue();
  expect(await nativeCommand(["open-note", link, "native-password"])).toBe(
    "browser to native note",
  );
  await page.goto(link);
  await expect(
    page.getByRole("heading", { name: "Transfer unavailable" }),
  ).toBeVisible();
});
