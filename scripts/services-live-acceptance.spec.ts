import { spawn } from "node:child_process";
import { once } from "node:events";
import { expect, test } from "@playwright/test";

const native = process.env.NATIVE_ACCEPTANCE_BINARY!;

async function acceptRisk(
  page: import("@playwright/test").Page,
): Promise<void> {
  const dialog = page.getByRole("dialog", { name: "WebRTC privacy" });
  if (await dialog.isVisible().catch(() => false)) {
    await dialog.getByRole("button", { name: "Accept and continue" }).click();
  }
}

async function sender(
  text: string,
  password: string,
): Promise<{ link: string; stop(): Promise<void> }> {
  const child = spawn(native, ["serve-live", text, password, "burn"], {
    env: process.env,
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  child.stderr.on("data", (data) => (stderr += data));
  const link = await new Promise<string>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`live sender did not publish: ${stderr}`)),
      45_000,
    );
    child.stdout.once("data", (data) => {
      clearTimeout(timer);
      resolve(String(data).trim());
    });
    child.once("exit", (code) =>
      reject(new Error(`live sender exited ${code}: ${stderr}`)),
    );
  });
  return {
    link,
    async stop() {
      child.stdin.end();
      await once(child, "exit");
    },
  };
}

test("browser first live burn claimant reads, and a wrong password does not burn", async ({
  browser,
}) => {
  const live = await sender("browser first live note", "live-browser-password");
  try {
    const wrong = await browser.newPage();
    await wrong.goto(live.link);
    await wrong.locator("#transfer-password").fill("wrong-live-password");
    await wrong.getByRole("button", { name: "Unlock" }).click();
    await expect(wrong.getByRole("alert")).toBeVisible();
    await wrong.close();

    const page = await browser.newPage();
    await page.goto(live.link);
    await page.locator("#transfer-password").fill("live-browser-password");
    await page.getByRole("button", { name: "Unlock" }).click();
    const consumed = page.waitForRequest("**/api/v1/transfers/*/consume");
    await page.getByRole("button", { name: "Decrypt note" }).click();
    await acceptRisk(page);
    await expect(page.getByText("browser first live note")).toBeVisible({
      timeout: 90_000,
    });
    await expect(
      page.getByText(
        "The live share was revoked. Your decrypted copy remains in this tab.",
      ),
    ).toBeVisible();
    const request = await consumed;
    const replay = await page.request.post(request.url(), {
      data: {},
      headers: {
        "X-Filebeam-Read-Token": request.headers()["x-filebeam-read-token"],
        "X-Filebeam-Session-Token":
          request.headers()["x-filebeam-session-token"],
      },
    });
    expect(replay.status()).not.toBe(202);
    await page.close();
  } finally {
    await live.stop();
  }
});
