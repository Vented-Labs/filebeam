import { expect, test, type Page } from '@playwright/test';
import { visual } from '../../tests/theme/visual';

async function probe(page: Page) {
    return page.evaluate(async () => {
        const url = '/editor-probe.ts';
        return (await import(/* @vite-ignore */ url)).inspect();
    });
}

test('the sole mode compartment preserves editor state through mode, OS and preset changes', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
    await page.goto('/?theme');
    const content = page.locator('.cm-content');
    await expect(content).toBeVisible();
    const element = await page.locator('.cm-editor').elementHandle();
    await content.fill('const retained = 42;\n'.repeat(80));
    await content.press('Control+End');
    await page.keyboard.insertText('\n// retained undo step');
    await page.evaluate(async () => {
        const url = '/editor-probe.ts';
        (await import(/* @vite-ignore */ url)).select(304, 314);
        await new Promise<void>((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        );
        (await import(/* @vite-ignore */ url)).scroll(300);
        await new Promise<void>((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        );
    });
    const initial = await probe(page);
    expect(initial.dark).toBe(true);
    expect(initial.undo).toBeGreaterThan(0);
    expect(initial.scroll).toBeGreaterThan(0);
    for (const mode of ['light', 'dark'] as const) {
        await page.evaluate((next) => window.filebeamAppearance!.set(next), mode);
        await expect.poll(async () => (await probe(page)).dark).toBe(mode === 'dark');
        const next = await probe(page);
        expect({ ...next, dark: initial.dark }).toEqual(initial);
        expect(await element!.evaluate((node) => node.isConnected)).toBe(true);
    }
    await page.evaluate(() => window.filebeamAppearance!.set('system'));
    await page.emulateMedia({ colorScheme: 'light' });
    await expect.poll(async () => (await probe(page)).dark).toBe(false);
    await page.evaluate(() => window.filebeamAppearance!.setPreset('amber'));
    expect({ ...(await probe(page)), dark: initial.dark }).toEqual(initial);
    await page.evaluate(async () => {
        const url = '/editor-probe.ts';
        (await import(/* @vite-ignore */ url)).history('undo');
    });
    await expect(content).not.toContainText('retained undo step');
    await page.evaluate(async () => {
        const url = '/editor-probe.ts';
        (await import(/* @vite-ignore */ url)).history('redo');
    });
    await expect(content).toContainText('retained undo step');
    await page.getByRole('switch', { name: 'Wrap', exact: true }).click();
    expect((await probe(page)).wrap).toBe(false);
    await page.getByRole('switch', { name: 'Read only', exact: true }).click();
    expect((await probe(page)).readOnly).toBe(true);
    await expect(content).toHaveAttribute('contenteditable', 'false');
    await expect(page.locator('.note-composer__body')).toHaveCSS('opacity', '1');
});

test('the explicit highlighter covers every supported semantic tag', async ({ page }) => {
    await page.goto('/?theme');
    await expect(page.locator('.cm-content')).toBeVisible();
    const classes = await page.evaluate(async () => {
        const url = '/editor-probe.ts';
        return (await import(/* @vite-ignore */ url)).tagClasses();
    });
    for (const [role, value] of Object.entries(classes)) expect(value).toBe(`fb-syn-${role}`);
});

const languages = [
    { label: 'Plain text', source: 'const value = 42; // remains plain', roles: [] },
    {
        label: 'PHP',
        source: '<?php\n// Private message\nfunction send(string $name): int { return strlen("hello") + 42; }',
        roles: ['keyword', 'comment', 'string', 'number', 'function'],
    },
    {
        label: 'JavaScript',
        source: '// Private message\nclass Parcel { send(value = 42) { return /safe/i.test("hello\\nworld") && true ? { value, empty: null } : false; } }\nnew Parcel().send();',
        roles: [
            'keyword',
            'comment',
            'string',
            'number',
            'function',
            'property',
            'constant',
            'regexp',
            'operator',
        ],
    },
    {
        label: 'TypeScript',
        source: '// Private message\ntype Parcel = { size: number };\nconst size: number = 42;\nfunction send(value: Parcel): string { return "hello"; }\nsend({ size });',
        roles: ['keyword', 'type', 'comment', 'string', 'number', 'function', 'property'],
    },
    {
        label: 'JSON',
        source: '{"message": "hello", "count": 42, "ready": true, "empty": null}',
        roles: ['property', 'string', 'number', 'constant'],
    },
    {
        label: 'Markdown',
        source: '# Private heading\n\n*emphasis* **strong** [link](https://example.test) `code`\n\n```js\nconst ready = true;\n```',
        roles: ['heading', 'emphasis', 'strong', 'link', 'monospace'],
    },
    {
        label: 'CSS',
        source: '/* Private style */\n.card { color: red; width: 42px; font-family: "Inter"; }',
        roles: ['comment', 'property', 'number', 'string'],
    },
    {
        label: 'HTML',
        source: '<!-- Private markup -->\n<section class="card"><strong>Hello</strong></section>',
        roles: ['comment', 'tag', 'attribute', 'string'],
    },
    {
        label: '.ENV',
        source: '# Private configuration\nMESSAGE="hello"\nCOUNT=42',
        roles: ['comment', 'keyword', 'string'],
    },
];

for (const language of languages) {
    test(`actual ${language.label} parser output uses fixed readable syntax in both modes`, async ({
        page,
    }) => {
        await page.goto('/?theme');
        await page.getByRole('combobox', { name: 'Note language' }).click();
        await page.getByRole('option', { name: language.label, exact: true }).click();
        await page.locator('.cm-content').fill(language.source);
        if (language.label === 'Plain text')
            await expect(page.locator('[class*="fb-syn-"]')).toHaveCount(0);
        for (const mode of ['light', 'dark'] as const) {
            await page.evaluate((next) => window.filebeamAppearance!.set(next), mode);
            for (const role of language.roles) {
                const token = page.locator(`.cm-editor .fb-syn-${role}`).first();
                await expect(token).toBeVisible();
                if (['emphasis', 'strong', 'monospace'].includes(role)) continue;
                expect(
                    await token.evaluate((element, name) => {
                        const probe = document.createElement('span');
                        probe.style.color = `var(--fb-editor-${name})`;
                        element.append(probe);
                        const expected = getComputedStyle(probe).color;
                        probe.remove();
                        return getComputedStyle(element).color === expected;
                    }, role),
                ).toBe(true);
            }
            await expect.poll(async () => (await probe(page)).dark).toBe(mode === 'dark');
        }
    });
}

test('loading failures retain a selectable textarea and large drafts avoid parsing', async ({
    page,
}) => {
    await page.route('**/ui/src/lib/editor-theme.ts*', (route) => route.abort());
    await page.goto('/?theme');
    const fallback = page.getByRole('textbox', { name: 'Secure note editor' });
    await expect(fallback).toHaveJSProperty('tagName', 'TEXTAREA');
    await fallback.fill('fallback draft');
    await fallback.press('Control+A');
    expect(
        await fallback.evaluate(
            (node: HTMLTextAreaElement) => node.selectionEnd - node.selectionStart,
        ),
    ).toBe(14);
    await page.unroute('**/ui/src/lib/editor-theme.ts*');
    await page.goto('/?theme&large');
    await expect(page.locator('.cm-content')).toBeVisible();
    expect((await probe(page)).document.length).toBe(250_001);
    await expect(page.locator('[class*="fb-syn-"]')).toHaveCount(0);
});

test('search and keyboard selections remain visible above the translucent active line', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/?theme');
    await expect(page.locator('.cm-content')).toBeVisible();
    for (const mode of ['light', 'dark'] as const) {
        await page.evaluate(async (next) => {
            window.filebeamAppearance!.set(next);
            const url = '/editor-probe.ts';
            const probe = await import(/* @vite-ignore */ url);
            probe.select(0, 18);
            probe.search('Parcel');
        }, mode);
        await expect(page.locator('.cm-selectionBackground').first()).toBeVisible();
        await expect(page.locator('.cm-searchMatch').first()).toBeVisible();
        await visual(page.locator('.note-composer')).toHaveScreenshot(`selection-${mode}.png`, {
            animations: 'disabled',
        });
    }
});
