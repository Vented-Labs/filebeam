import { expect, test, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

async function paste(page: Page, text: string, images = 0, selector = 'body') {
    return page.locator(selector).evaluate(
        (target, { text, images }) => {
            const data = new DataTransfer();
            data.setData('text/plain', text);
            for (let i = 0; i < images; i++)
                data.items.add(
                    new File([new Uint8Array([137, 80, 78, 71])], `paste-${i}.png`, {
                        type: 'image/png',
                    }),
                );
            const event = new ClipboardEvent('paste', {
                clipboardData: data,
                bubbles: true,
                cancelable: true,
            });
            target.dispatchEvent(event);
            return event.defaultPrevented;
        },
        { text, images },
    );
}

async function noteText(page: Page) {
    return page.getByTestId('note-editor').evaluate((editor) => {
        const fallback = editor.querySelector('textarea');
        return fallback
            ? fallback.value
            : Array.from(editor.querySelectorAll('.cm-line'))
                  .map((line) => line.textContent)
                  .join('\n');
    });
}

test('generic paste preserves notes, selects files once, and leaves fields alone', async ({
    page,
}) => {
    await page.goto('/');
    await paste(page, 'first 日本\n\tsecond');
    await expect(page.getByRole('tab', { name: 'Notes' })).toHaveAttribute('aria-selected', 'true');
    await expect.poll(() => noteText(page)).toContain('first 日本');
    await expect(
        page.getByRole('textbox', { name: 'Secure note editor', exact: true }),
    ).toBeFocused();
    expect(await paste(page, 'title', 0, '#note-title')).toBe(false);
    await paste(page, 'image fallback should not become a note', 2);
    await expect(page.getByRole('tab', { name: 'Files', exact: true })).toHaveAttribute(
        'aria-selected',
        'true',
    );
    await expect(page.getByText('paste-0.png', { exact: true })).toHaveCount(1);
    await expect(page.getByText('paste-1.png', { exact: true })).toHaveCount(1);
    await paste(page, '\nthird');
    await expect.poll(() => noteText(page)).toContain('third');
    expect(await noteText(page)).not.toContain('image fallback');
    await page.getByRole('tab', { name: 'Files', exact: true }).click();
    await expect(page.getByText('paste-0.png', { exact: true })).toBeVisible();
});

test('paste-anywhere has no dedicated button or modal and leaves other popups alone', async ({
    page,
}) => {
    await page.goto('/');
    await expect(page.getByRole('button', { name: 'Paste', exact: true })).toHaveCount(0);
    await expect(page.getByRole('dialog', { name: 'Paste from your clipboard' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    expect(await paste(page, 'ignore outside dialog')).toBe(false);
    await page.keyboard.press('Escape');
    await expect(page.locator('[role="dialog"]')).toHaveCount(0);
    await paste(page, 'ordinary page paste');
    await expect.poll(() => noteText(page)).toBe('ordinary page paste');
});

test('empty and HTML-only paste preserve the draft', async ({ page }) => {
    await page.goto('/');
    expect(await paste(page, '')).toBe(false);
    await expect(page.getByRole('tab', { name: 'Files', exact: true })).toHaveAttribute(
        'aria-selected',
        'true',
    );
    await page.evaluate(() => {
        const data = new DataTransfer();
        data.setData('text/html', '<img src="https://example.com/image.png">');
        document.body.dispatchEvent(
            new ClipboardEvent('paste', { clipboardData: data, bubbles: true }),
        );
    });
    await expect(page.getByRole('heading', { name: 'Drop your files here' })).toBeVisible();
});

test('pasted multiline note round-trips without sending plaintext', async ({ page, browser }) => {
    const text = 'PASTED_PRIVATE_NOTE_日本\n\tsecond line\n';
    await page.goto('/');
    await paste(page, text);
    const requests: string[] = [];
    page.on('request', (request) => {
        if (request.url().includes('/api/v1/transfers')) requests.push(request.postData() ?? '');
    });
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' && response.url().endsWith('/api/v1/transfers'),
    );
    await page.getByRole('button', { name: 'Send encrypted', exact: true }).click();
    const { data } = await (await creation).json();
    const recipient = await browser.newContext();
    try {
        await expect(
            page.getByRole('heading', { name: 'Your encrypted link is ready' }),
        ).toBeVisible();
        expect(await paste(page, 'must not change a completed note')).toBe(false);
        expect(requests.join('\n')).not.toContain('PASTED_PRIVATE_NOTE');
        const receiver = await recipient.newPage();
        await receiver.goto(await page.locator('#share-link').inputValue());
        await receiver.getByRole('button', { name: 'Decrypt note' }).click();
        await expect.poll(() => noteText(receiver)).toBe(text);
    } finally {
        await recipient.close();
        await page.request.delete(`/api/v1/transfers/${data.id}`, {
            headers: { 'X-Filebeam-Delete-Token': data.delete_token },
        });
    }
});

test('real browser clipboard image round-trips as an encrypted file', async ({
    page,
    context,
    browser,
}) => {
    await page.goto('/');
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const original = await page.evaluate(async () => {
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = 2;
        canvas.getContext('2d')!.fillRect(0, 0, 2, 2);
        const blob = await new Promise<Blob>((resolve) =>
            canvas.toBlob((blob) => resolve(blob!), 'image/png'),
        );
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
        // Browsers may re-encode images when writing to the OS clipboard.
        const [item] = await navigator.clipboard.read();
        const clipboardImage = await item.getType('image/png');
        return Array.from(new Uint8Array(await clipboardImage.arrayBuffer()));
    });
    await page.getByRole('heading', { name: 'Drop your files here' }).click();
    await page.keyboard.press('ControlOrMeta+v');
    await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' && response.url().endsWith('/api/v1/transfers'),
    );
    await page.getByRole('button', { name: 'Send encrypted', exact: true }).click();
    const { data } = await (await creation).json();
    const recipient = await browser.newContext();
    await recipient.addInitScript(() =>
        Object.defineProperty(window, 'showSaveFilePicker', {
            value: undefined,
            configurable: true,
        }),
    );
    try {
        await expect(
            page.getByRole('heading', { name: 'Your encrypted link is ready' }),
        ).toBeVisible();
        const receiver = await recipient.newPage();
        await receiver.goto(await page.locator('#share-link').inputValue());
        const download = receiver.waitForEvent('download');
        await receiver.getByRole('button', { name: 'Download files', exact: true }).click();
        const saved = await download;
        expect(await readFile((await saved.path())!)).toEqual(Buffer.from(original));
    } finally {
        await recipient.close();
        await page.request.delete(`/api/v1/transfers/${data.id}`, {
            headers: { 'X-Filebeam-Delete-Token': data.delete_token },
        });
    }
});
