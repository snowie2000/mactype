import { expect, test } from "@playwright/test";
import path from "node:path";
import { galleryLocales, galleryViews } from "./windows";

const galleryRoot = path.resolve(__dirname, "../../artifacts/frontend-gallery");

async function overflowingElements(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const viewportWidth = document.documentElement.clientWidth;
    const isClippedByAncestor = (element: HTMLElement) => {
      let parent = element.parentElement;
      while (parent && parent !== document.body) {
        const overflowX = window.getComputedStyle(parent).overflowX;
        if (["auto", "scroll", "hidden", "clip"].includes(overflowX)) return true;
        parent = parent.parentElement;
      }
      return false;
    };
    return [...document.querySelectorAll<HTMLElement>("body *")]
      .map((element) => ({ element, rect: element.getBoundingClientRect() }))
      .filter(({ element, rect }) => (rect.right > viewportWidth + 1 || rect.left < -1) && !isClippedByAncestor(element))
      .map(({ element, rect }) => `${element.tagName.toLowerCase()}.${element.className || "-"} [${Math.round(rect.left)}, ${Math.round(rect.right)}]`)
      .slice(0, 12);
  });
}

async function openServiceDetails(page: import("@playwright/test").Page) {
  const rows = page.locator("details.service-row");
  for (let index = 0; index < await rows.count(); index += 1) {
    const row = rows.nth(index);
    if (!await row.evaluate((element) => (element as HTMLDetailsElement).open)) {
      await row.locator("summary").click();
    }
  }
}

const executionStateGallery = [
  { id: "ready", query: "system-service=ready", expected: "Running" },
  { id: "degraded", query: "system-service=degraded", expected: "Running" },
  { id: "initializing", query: "system-service=initializing", expected: "Running" },
  { id: "health-unknown", query: "system-service=unknown-health", expected: "Running" },
  { id: "stopped", query: "system-service=ready&service-runtime=stopped", expected: "Stopped" },
  { id: "starting", query: "system-service=ready&service-runtime=start-pending", expected: "Starting" },
  { id: "stopping", query: "system-service=ready&service-runtime=stop-pending", expected: "Stopping" },
  { id: "paused", query: "system-service=ready&service-runtime=paused", expected: "Paused" },
  { id: "runtime-unknown", query: "system-service=ready&service-runtime=unknown", expected: "Unknown state" },
  { id: "failed", query: "system-service=failed", expected: "Service configuration needs repair." },
  { id: "outdated", query: "system-service=outdated", expected: "Update required" },
  { id: "profile-mismatch", query: "system-service=profile-mismatch", expected: "Service running with a different profile" },
  { id: "not-installed", query: "system-service=migration-available", expected: "Install service" },
  { id: "foreign-service", query: "system-service=foreign-service", expected: "unexpected configuration" },
  { id: "inaccessible-service", query: "system-service=inaccessible-service", expected: "Inaccessible" },
  { id: "removal-pending", query: "system-service=delete-pending", expected: "Removal pending" },
  { id: "appinit-running", query: "system-service=legacy-conflict&legacy=migration-available&raw-active=1", expected: "Service running while AppInit conflicts" },
  { id: "appinit-stopped", query: "system-service=legacy-conflict&service-runtime=stopped", expected: "AppInit registry mode is active" },
  { id: "legacy-migration-running", query: "system-service=migration-available&legacy=migration-available&legacy-state=running", expected: "Legacy MacTray was detected." },
  { id: "legacy-migration-stopped", query: "system-service=migration-available&legacy=migration-available&legacy-state=stopped", expected: "Legacy MacTray was detected." },
  { id: "legacy-transition", query: "system-service=migration-available&legacy=migration-available&legacy-state=start-pending", expected: "A legacy MacTray service must be resolved first" },
  { id: "legacy-foreign", query: "system-service=migration-available&legacy=foreign", expected: "A foreign legacy MacTray service was detected" },
  { id: "legacy-uncertain", query: "system-service=migration-available&legacy=inaccessible", expected: "Legacy MacTray service status could not be verified" },
  { id: "mactray-current-session", query: "system-service=migration-available&legacy-tray=trusted-current", expected: "Existing MacTray is running" },
  { id: "mactray-other-session", query: "system-service=migration-available&legacy-tray=trusted-other", expected: "MacTray is running in another user session" },
  { id: "mactray-untrusted-process", query: "system-service=migration-available&legacy-tray=untrusted", expected: "A same-named process could not be trusted" },
  { id: "mactray-process-unknown", query: "system-service=migration-available&legacy-tray=unknown", expected: "MacTray tray mode status is unavailable" },
  { id: "mactray-autostart", query: "system-service=migration-available&legacy-startup=hkcu-run", expected: "MacTray autostart must be disabled" },
  { id: "mactray-autostart-untrusted", query: "system-service=migration-available&legacy-startup=untrusted", expected: "A MacTray autostart entry could not be trusted" },
  { id: "mactray-autostart-unknown", query: "system-service=migration-available&legacy-startup=unknown", expected: "MacTray autostart status is unavailable" },
  { id: "native-running-with-mactray", query: "system-service=ready&legacy-tray=trusted-current", expected: "Existing MacTray is running" },
] as const;

for (const state of executionStateGallery) {
  test(`execution state gallery captures ${state.id}`, async ({ page }, testInfo) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&${state.query}`, { waitUntil: "networkidle" });

    const summary = page.locator("[data-service-summary]");
    await expect(summary).toContainText(state.expected);
    await expect(page.locator("details.service-row[open]")).toHaveCount(0);
    expect(await overflowingElements(page)).toEqual([]);
    await page.screenshot({
      path: path.join(galleryRoot, `${testInfo.project.name}-execution-state-${state.id}-en.png`),
      fullPage: true,
    });
  });
}

for (const wording of [
  { locale: "ko", expected: "새 서비스", forbidden: "신식 서비스" },
  { locale: "zh-CN", expected: "新服务", forbidden: "新式服务" },
  { locale: "zh-TW", expected: "新服務", forbidden: "新式服務" },
] as const) {
  test(`service terminology is natural in ${wording.locale}`, async ({ page }) => {
    await page.goto(`/?view=execution&gallery=1&lang=${wording.locale}&system-service=ready`, { waitUntil: "networkidle" });
    const main = page.locator("main");
    await expect(main).toContainText(wording.expected);
    await expect(main).not.toContainText(wording.forbidden);
  });
}

for (const view of galleryViews) {
  for (const locale of galleryLocales) {
    test(`${view.id} renders fully in ${locale.id}`, async ({ page }, testInfo) => {
      const failures: string[] = [];
      page.on("console", (message) => {
        if (message.type() === "error") failures.push(`console: ${message.text()}`);
      });
      page.on("pageerror", (error) => failures.push(`pageerror: ${error.message}`));
      page.on("crash", () => failures.push("renderer process crashed"));

      await page.goto(`/?view=${view.id}&gallery=1&lang=${locale.id}`, { waitUntil: "networkidle" });
      await expect(page.locator("html")).toHaveAttribute("lang", locale.id);
      await expect(page.locator("html")).toHaveAttribute("dir", locale.direction);
      await expect(page.locator("body")).toHaveAttribute("data-rendered", "true");
      await expect(page.locator("body")).toHaveAttribute("data-view", view.id);
      await expect(page.locator("body")).toHaveAttribute("data-locale", locale.id);
      await expect(page.getByRole("heading", { level: 1, name: view.title[locale.id] })).toBeVisible();
      expect(await page.locator("main").innerText()).toMatch(locale.script);
      expect(await overflowingElements(page), `${locale.id} view must not overflow horizontally`).toEqual([]);
      expect(failures, failures.join("\n")).toEqual([]);

      await page.screenshot({
        path: path.join(galleryRoot, `${testInfo.project.name}-${view.id}-${locale.id}.png`),
        fullPage: true,
      });
    });
  }
}

test("profile editor categories and collections remain interactive", async ({ page }, testInfo) => {
  const failures: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") failures.push(`console: ${message.text()}`);
  });
  page.on("pageerror", (error) => failures.push(`pageerror: ${error.message}`));

  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });
  const undo = page.getByRole("button", { name: "되돌리기", exact: true });
  const redo = page.getByRole("button", { name: "다시 하기", exact: true });
  const discard = page.getByRole("button", { name: "변경 취소", exact: true });
  await expect(undo).toBeDisabled();
  const firstSelect = page.locator(".setting-row select").first();
  const initialOption = await firstSelect.inputValue();
  const nextOption = await firstSelect.locator("option").evaluateAll((options, current) => options.map((option) => (option as HTMLOptionElement).value).find((value) => value !== current), initialOption);
  if (!nextOption) throw new Error("The first profile setting must expose an alternate option");
  await firstSelect.selectOption(nextOption);
  await expect(undo).toBeEnabled();
  await undo.click();
  await expect(redo).toBeEnabled();
  await redo.click();
  await expect(page.getByRole("button", { name: "지금 적용" })).toBeDisabled();
  await page.getByRole("button", { name: "지금 저장" }).click();
  await expect(page.locator(".profile-message")).toContainText("지금 저장했습니다");
  await expect(page.getByRole("button", { name: "지금 적용" })).toBeEnabled();
  await page.getByRole("button", { name: "지금 적용" }).click();
  await expect(page.locator(".profile-message")).toContainText("실제 MacType 시스템 범위에 적용했습니다");
  await firstSelect.selectOption(initialOption);
  await expect(discard).toBeEnabled();
  await discard.click();
  await expect(discard).toBeDisabled();
  await firstSelect.selectOption(initialOption);
  await page.getByRole("button", { name: "지금 저장" }).click();
  await expect(page.locator(".profile-message")).toContainText("지금 저장했습니다");
  await expect(discard).toBeDisabled();

  // The default stack renders the sample once, because a second sample group
  // would claim the height the settings form needs. Wide layouts dock the
  // preview beside the form; narrower ones keep a bottom panel that no longer
  // grows into the form.
  await expect(page.locator(".preview-strip img")).toHaveCount(1);
  const previewResizer = page.getByRole("separator", { name: "프리뷰 영역 높이 조절" });
  const docked = await page.locator(".settings-workspace").getAttribute("data-preview-docked") === "true";
  if (docked) {
    await expect(previewResizer).toHaveCount(0);
    const formBox = await page.locator(".settings-form").boundingBox();
    const panelBox = await page.locator(".preview-panel").boundingBox();
    if (!formBox || !panelBox) throw new Error("The docked layout must show both columns");
    expect(panelBox.x, "the docked preview sits beside the form, not under it").toBeGreaterThanOrEqual(formBox.x + formBox.width - 1);
  } else {
    const settledHeight = Number(await previewResizer.getAttribute("aria-valuenow"));
    expect(settledHeight, "footer control must stay visible").toBeGreaterThanOrEqual(220);
    expect(settledHeight, "the panel must not grow past its default").toBeLessThanOrEqual(300);
    await previewResizer.press("ArrowDown");
    await expect(previewResizer).toHaveAttribute("aria-valuenow", String(settledHeight - 16));
    await previewResizer.press("Home");
    await expect(previewResizer).toHaveAttribute("aria-valuenow", "128");
  }

  await page.getByRole("button", { name: "LCD·픽셀 배열" }).click();
  await expect(page.getByRole("heading", { name: "LCD·픽셀 배열" })).toBeVisible();
  await expect(page.getByText("빨강 채널 튜닝", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "고급·실험" }).click();
  await expect(page.getByText("DirectWrite 감마", { exact: true })).toBeVisible();
  const shadow = page.getByRole("group", { name: "그림자 버퍼" });
  await shadow.getByRole("checkbox").check();
  await shadow.getByRole("spinbutton", { name: "가로 위치" }).fill("-2");
  await shadow.getByRole("spinbutton", { name: "세로 위치" }).fill("3");
  const lcdWeights = page.getByRole("group", { name: "사용자 지정 LCD 필터 가중치" });
  await lcdWeights.getByRole("checkbox").check();
  await expect(lcdWeights.getByRole("spinbutton")).toHaveCount(5);
  const pixelLayout = page.getByRole("group", { name: "사용자 지정 픽셀 배열" });
  await pixelLayout.getByRole("checkbox").check();
  await expect(pixelLayout.getByRole("spinbutton")).toHaveCount(6);
  const substitutionsBefore = await page.getByRole("combobox", { name: "원본 글꼴" }).count();
  await page.getByRole("button", { name: "글꼴 대체 추가" }).click();
  await expect(page.getByRole("combobox", { name: "원본 글꼴" })).toHaveCount(substitutionsBefore + 1);

  await page.getByRole("button", { name: "글꼴별 설정" }).click();
  await page.getByRole("combobox", { name: "설치된 글꼴 선택" }).selectOption("Arial");
  await expect(page.locator(".individual-row > strong").filter({ hasText: "Arial" })).toBeVisible();

  await page.getByRole("button", { name: "포함·제외" }).click();
  await page.getByRole("combobox", { name: "제외 글꼴 · 목록에 글꼴 추가" }).selectOption("Calibri");
  await expect(page.locator(".list-editor li > code").filter({ hasText: "Calibri" })).toBeVisible();
  await expect(page.getByText("제외 프로그램", { exact: true })).toBeVisible();
  await expect(page.getByText("주입 해제 DLL", { exact: true })).toBeVisible();
  await expect(page.getByText("글꼴 대체 제외 모듈", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "어두운 배경" }).click();
  await expect(page.getByRole("img", { name: "현재 설정의 글자 렌더링 프리뷰" })).toHaveAttribute("data-dark", "true");
  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
  expect(horizontalOverflow, "interactive profile editor must not have horizontal scrolling").toBe(false);
  expect(failures, failures.join("\n")).toEqual([]);
});

test("structured list editors add typed entries, reject duplicates, and suggest running processes", async ({ page }, testInfo) => {
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "포함·제외" }).click();

  const excludePrograms = page.locator(".list-editor").filter({ hasText: "제외 프로그램" });
  const entryInput = excludePrograms.getByRole("combobox", { name: "제외 프로그램 · 추가" });
  await expect(excludePrograms.locator("li > code").filter({ hasText: "fontview.exe" })).toBeVisible();

  await entryInput.fill("notepad.exe");
  await entryInput.press("Enter");
  await expect(excludePrograms.locator("li > code").filter({ hasText: "notepad.exe" })).toBeVisible();
  await expect(entryInput).toHaveValue("");

  await entryInput.fill("NOTEPAD.EXE");
  await excludePrograms.getByRole("button", { name: "제외 프로그램 · 추가" }).click();
  await expect(excludePrograms.getByText("NOTEPAD.EXE은(는) 이미 목록에 있습니다.", { exact: true })).toBeVisible();
  await expect(excludePrograms.locator("li")).toHaveCount(2);

  await excludePrograms.getByRole("button", { name: "notepad.exe 제거" }).click();
  await expect(excludePrograms.locator("li")).toHaveCount(1);
  await expect(excludePrograms.getByText("이미 목록에 있습니다", { exact: false })).toHaveCount(0);

  await expect(entryInput).toHaveAttribute("list", "list-process-suggestions");
  await expect(page.locator("#list-process-suggestions option[value='code.exe']")).toHaveCount(1);

  const unloadDlls = page.locator(".list-editor").filter({ hasText: "주입 해제 DLL" });
  await expect(unloadDlls.getByText("아직 항목이 없습니다.", { exact: true })).toBeVisible();

  expect(await overflowingElements(page)).toEqual([]);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-list-editors-ko.png`), fullPage: true });
});

test("profile preview docks as a full-height right column at wide widths", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "Docked preview behavior is width-specific");
  await page.setViewportSize({ width: 1680, height: 900 });
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });

  await expect(page.locator('.settings-workspace[data-preview-docked="true"]')).toHaveCount(1);
  await expect(page.locator(".preview-resizer")).toHaveCount(0);
  const formBox = await page.locator(".settings-form").boundingBox();
  const previewBox = await page.locator(".preview-panel").boundingBox();
  expect(formBox).not.toBeNull();
  expect(previewBox).not.toBeNull();
  expect(previewBox!.x, "docked preview must sit right of the settings form").toBeGreaterThanOrEqual(formBox!.x + formBox!.width);
  expect(await overflowingElements(page)).toEqual([]);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-preview-docked-ko.png`), fullPage: true });
});

test("native preview display mode dropdown drives the runtime adapter", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "The preview footer is hidden on compact layouts");
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });

  const modeSelect = page.getByRole("combobox", { name: "표시 방식" });
  await expect(modeSelect).toBeVisible();
  await expect(modeSelect.locator("option")).toHaveText(["기본 표시", "나열 표시"]);

  const nativePreviewState = () => page.evaluate(() => window.sessionStorage.getItem("gallery-native-preview"));
  const nativePreviewBackground = () => page.evaluate(() => window.sessionStorage.getItem("gallery-native-preview-background"));
  await page.getByRole("button", { name: "실제 창에서 보기" }).click();
  await expect.poll(nativePreviewState).toBe("default");
  await expect.poll(nativePreviewBackground).toBe("#EEF1F4");
  // The background choice reaches the open window without reopening it, and it
  // survives a display-mode change.
  await page.getByRole("button", { name: "어두운 배경" }).click();
  await expect.poll(nativePreviewBackground).toBe("#171A1F");
  await modeSelect.selectOption("listing");
  await expect.poll(nativePreviewState).toBe("listing");
  await expect.poll(nativePreviewBackground).toBe("#171A1F");
  await page.getByRole("button", { name: "밝은 배경" }).click();
  await expect.poll(nativePreviewBackground).toBe("#EEF1F4");
  await page.getByRole("button", { name: "실제 창 닫기" }).click();
  await expect.poll(nativePreviewState).toBe("hidden");
  expect(await overflowingElements(page)).toEqual([]);
});

test("preview comparison renders the saved and edited sides only while edits exist", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "The preview toolbar wraps on compact layouts");
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });

  const compare = page.getByRole("button", { name: "비교", exact: true });
  const strips = page.locator(".preview-strip");
  await expect(compare).toBeDisabled();
  const baseline = await strips.count();
  expect(baseline).toBeGreaterThan(0);

  const firstSelect = page.locator(".setting-row select").first();
  const initialOption = await firstSelect.inputValue();
  const nextOption = await firstSelect.locator("option").evaluateAll((options, current) => options.map((option) => (option as HTMLOptionElement).value).find((value) => value !== current), initialOption);
  if (!nextOption) throw new Error("The first profile setting must expose an alternate option");
  await firstSelect.selectOption(nextOption);
  await expect(compare).toBeEnabled();

  await compare.click();
  await expect(compare).toHaveAttribute("aria-pressed", "true");
  await expect(strips).toHaveCount(baseline * 2);
  await expect(page.locator(".preview-strip figcaption").first()).toContainText("저장본");
  await expect(page.locator(".preview-strip figcaption").nth(1)).toContainText("편집본");
  expect(await overflowingElements(page)).toEqual([]);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-preview-compare-ko.png`), fullPage: true });

  // Saving makes both sides identical, so the comparison switches itself off.
  await page.getByRole("button", { name: "지금 저장" }).click();
  await expect(compare).toBeDisabled();
  await expect(compare).toHaveAttribute("aria-pressed", "false");
  await expect(strips).toHaveCount(baseline);
});

test("settings navigation restores the legacy Wizard and Tuner hierarchy", async ({ page }, testInfo) => {
  await page.goto("/?view=overview&gallery=1&lang=ko", { waitUntil: "networkidle" });

  const wizardGroup = page.locator(".navigation").getByRole("group", { name: "위자드" });
  const tunerGroup = page.locator(".navigation").getByRole("group", { name: "튜너" });
  await expect(wizardGroup.getByRole("button", { name: "프로필" })).toBeVisible();
  await expect(wizardGroup.getByRole("button", { name: "서비스" })).toBeVisible();
  await expect(tunerGroup.getByRole("button", { name: "단계별 설정" })).toBeVisible();
  await expect(tunerGroup.getByRole("button", { name: "전체 설정" })).toBeVisible();
  await expect(page.locator(".navigation").getByRole("button", { name: "위자드", exact: true })).toHaveCount(0);
  await expect(page.locator(".navigation").getByRole("button", { name: "튜너", exact: true })).toHaveCount(0);

  await tunerGroup.getByRole("button", { name: "단계별 설정" }).click();
  await expect(page.locator(".profile-page")).toHaveAttribute("data-mode", "quick");
  await expect(page.getByRole("heading", { level: 1, name: "단계별 설정" })).toBeVisible();
  await expect(page.locator(".profile-mode-title > span")).toHaveText("Tuner");
  expect(await page.locator(".profile-page").innerText()).not.toContain("마법사");
  await expect(page.locator(".settings-index button")).toHaveCount(9);
  await expect(page.locator(".settings-step")).toHaveCount(9);
  await expect(page.locator(".settings-index").getByRole("button", { name: "고급·실험" })).toHaveCount(0);
  await expect(page.getByRole("toolbar", { name: "프로필 편집 작업" })).toHaveCount(0);
  const settingsForm = page.locator(".settings-form");

  await expect(page.getByRole("heading", { level: 2, name: "시작" })).toBeVisible();
  await expect(page.locator(".wizard-start-card")).toBeVisible();
  await expect(page.locator(".wizard-start-profile code")).toBeVisible();
  await expect(page.locator(".wizard-start-font select")).toBeVisible();
  await expect(page.getByRole("button", { name: "이전" })).toHaveCount(0);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-guided-start-ko.png`), fullPage: true });

  await page.getByRole("button", { name: "진행" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "기본 렌더링" })).toBeVisible();
  await expect(page.getByRole("button", { name: "이전" })).toBeVisible();
  expect(await page.locator(".guided-choice").getByRole("radio").count()).toBeGreaterThanOrEqual(3);
  await expect(page.locator(".setting-actions")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "단계 기본값 복원" })).toBeVisible();
  expect(await settingsForm.evaluate((element) => element.scrollWidth > element.clientWidth), "Guided settings must not have internal horizontal scrolling").toBe(false);
  // The generic overflow gate skips anything inside an overflow-hidden
  // ancestor, so the workspace column needs its own window-bounds check: a
  // wide control in the preview toolbar used to inflate the column past the
  // right edge, clipping the step body instead of scrolling.
  for (const selector of [".settings-form", ".preview-panel", ".wizard-step-tools"]) {
    const bounds = await page.locator(selector).first().evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { left: Math.round(rect.left), right: Math.round(rect.right), viewport: document.documentElement.clientWidth };
    });
    expect(bounds.right, `${selector} must stay inside the window`).toBeLessThanOrEqual(bounds.viewport + 1);
    expect(bounds.left, `${selector} must not start off the left edge`).toBeGreaterThanOrEqual(-1);
  }
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-guided-rendering-ko.png`), fullPage: true });

  await page.getByRole("button", { name: "진행" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "글꼴 품질" })).toBeVisible();
  await expect(page.locator(".guided-scale-words").first()).toContainText("가늘게");

  // Restored legacy Tuner screen: bold and italic together, previewed as
  // bold, italic, and bold italic lines of the same pangram.
  await page.getByRole("button", { name: "진행" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "굵게·기울임" })).toBeVisible();
  const guidedLabels = page.locator(".guided-label label");
  await expect(guidedLabels.nth(0)).toHaveText("굵은 글자 굵기");
  await expect(guidedLabels.nth(1)).toHaveText("굵게 처리 방식");
  await expect(guidedLabels.nth(2)).toHaveText("기울임 정도");
  await expect(page.locator(".preview-strip")).toHaveCount(3);
  await expect(page.locator(".preview-strip figcaption")).toHaveText(["굵게", "기울임", "굵은 기울임"]);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-guided-bold-italic-ko.png`), fullPage: true });

  // Restored legacy Tuner screen: contrast then gamma sliders before the mode.
  // The step is titled by what it does, not by the core setting it carries.
  await page.locator(".settings-index").getByRole("button", { name: "밝기와 대비" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "밝기와 대비" })).toBeVisible();
  await expect(guidedLabels.nth(0)).toHaveText("대비");
  await expect(guidedLabels.nth(1)).toHaveText("감마 값");
  await expect(guidedLabels.nth(2)).toHaveText("감마 방식");

  // LCD screen gains the RGB text tuning and compares the current method
  // against the red, green, and blue channels without clipping line four.
  await page.locator(".settings-index").getByRole("button", { name: "LCD 배열과 색조" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "LCD 배열과 색조" })).toBeVisible();
  await expect(guidedLabels.nth(2)).toHaveText("빨강 채널 튜닝");
  await expect(page.locator(".preview-strip")).toHaveCount(4);
  await expect(page.locator(".preview-strip figcaption")).toHaveText(["현재 방식", "R", "G", "B"]);
  // Four lines scroll inside the stack instead of stretching the panel into
  // the step body, so line four is reachable and the step keeps its room. A
  // desktop window docks the guided preview beside the step rather than under it.
  await page.locator(".preview-strip").last().scrollIntoViewIfNeeded();
  await expect(page.locator(".preview-strip").last()).toBeInViewport();
  const stepBox = await page.locator(".wizard-step-content").boundingBox();
  if (!stepBox) throw new Error("The guided step body must stay visible");
  expect(stepBox.height, "the step body keeps its room beside the four-line stack").toBeGreaterThanOrEqual(240);
  if (testInfo.project.name === "desktop-1280") {
    await expect(page.locator(".settings-workspace")).toHaveAttribute("data-preview-docked", "true");
    await expect(page.getByRole("separator", { name: "프리뷰 영역 높이 조절" })).toHaveCount(0);
  }
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-guided-lcd-channels-ko.png`), fullPage: true });

  await page.locator(".settings-index").getByRole("button", { name: "힌팅" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "힌팅" })).toBeVisible();
  await page.locator(".settings-index").getByRole("button", { name: "적용 및 미리보기" }).click();
  await expect(page.getByRole("button", { name: "진행" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "MacType에 적용" })).toBeVisible();
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-guided-apply-ko.png`), fullPage: true });

  await tunerGroup.getByRole("button", { name: "전체 설정" }).click();
  await expect(page.locator(".profile-page")).toHaveAttribute("data-mode", "advanced");
  await expect(page.getByRole("heading", { level: 1, name: "전체 설정" })).toBeVisible();
  await expect(page.locator(".settings-index button")).toHaveCount(6);
  expect(await settingsForm.evaluate((element) => element.scrollWidth > element.clientWidth), "Tuner settings must not have internal horizontal scrolling").toBe(false);
  await expect(page.getByRole("checkbox", { name: "고급 설정 표시" })).toHaveCount(0);

  await wizardGroup.getByRole("button", { name: "프로필" }).click();
  await expect(page.locator("body")).toHaveAttribute("data-view", "files");
  await expect(page.getByRole("heading", { level: 1, name: "프로필" })).toBeVisible();
  await wizardGroup.getByRole("button", { name: "서비스" }).click();
  await expect(page.locator("body")).toHaveAttribute("data-view", "execution");
});

test("guided step undo, redo, and discard stay scoped to the current step", async ({ page }) => {
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.locator(".navigation").getByRole("button", { name: "단계별 설정" }).click();
  await expect(page.locator(".profile-page")).toHaveAttribute("data-mode", "quick");

  const undoStep = page.getByRole("button", { name: "되돌리기", exact: true });
  const redoStep = page.getByRole("button", { name: "다시 하기", exact: true });
  const discardStep = page.getByRole("button", { name: "단계 변경 취소", exact: true });

  // Steps without schema settings (start, substitution, apply) expose no step tools.
  await expect(page.getByRole("toolbar", { name: "단계 편집 작업" })).toHaveCount(0);
  await page.locator(".settings-index").getByRole("button", { name: "글꼴 대체" }).click();
  await expect(page.getByRole("toolbar", { name: "단계 편집 작업" })).toHaveCount(0);

  await page.locator(".settings-index").getByRole("button", { name: "글꼴 품질" }).click();
  const weightValue = page.locator("#normal_weight-value");
  await expect(undoStep).toBeDisabled();
  await expect(redoStep).toBeDisabled();
  await expect(discardStep).toBeDisabled();

  await weightValue.fill("24");
  await weightValue.press("Enter");
  await expect(weightValue).toHaveValue("24");
  await expect(discardStep).toBeEnabled();

  await undoStep.click();
  await expect(weightValue).toHaveValue("0");
  await expect(undoStep).toBeDisabled();
  await redoStep.click();
  await expect(weightValue).toHaveValue("24");
  await expect(redoStep).toBeDisabled();

  // Ctrl+Z / Ctrl+Y drive the same step-scoped history from the keyboard.
  await expect(undoStep).toBeEnabled();
  await page.keyboard.press("Control+z");
  await expect(weightValue).toHaveValue("0");
  await expect(redoStep).toBeEnabled();
  await page.keyboard.press("Control+y");
  await expect(weightValue).toHaveValue("24");

  // The brightness step starts with an empty history even though the quality
  // step recorded edits, and its discard leaves the quality step untouched.
  await page.locator(".settings-index").getByRole("button", { name: "밝기와 대비" }).click();
  const contrastValue = page.locator("#contrast-value");
  await expect(undoStep).toBeDisabled();
  await expect(redoStep).toBeDisabled();
  await expect(discardStep).toBeDisabled();

  await contrastValue.fill("2");
  await contrastValue.press("Enter");
  await expect(contrastValue).toHaveValue("2");
  await discardStep.click();
  await expect(contrastValue).toHaveValue("1");
  await expect(discardStep).toBeDisabled();

  // The step discard itself is one more undoable step edit.
  await undoStep.click();
  await expect(contrastValue).toHaveValue("2");

  await page.locator(".settings-index").getByRole("button", { name: "글꼴 품질" }).click();
  await expect(weightValue).toHaveValue("24");
  await expect(undoStep).toBeEnabled();
  await expect(discardStep).toBeEnabled();
});

test("slider drags and exact number edits create one undo revision per interaction", async ({ page }, testInfo) => {
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "글자 모양", exact: true }).click();

  const weightRow = page.locator(".setting-row").filter({ hasText: "일반 글자 굵기" });
  const weightSlider = weightRow.locator('input[type="range"]');
  const exactWeight = weightRow.locator('input[type="number"]');
  const undo = page.getByRole("button", { name: "되돌리기", exact: true });
  const redo = page.getByRole("button", { name: "다시 하기", exact: true });
  await expect(weightSlider).toHaveCount(1);
  await expect(exactWeight).toHaveValue("0");
  const shapeLayout = await page.locator(".settings-form").evaluate((element) => ({ clientWidth: element.clientWidth, scrollWidth: element.scrollWidth }));
  expect(shapeLayout.scrollWidth, "slider rows must fit without hidden horizontal overflow").toBeLessThanOrEqual(shapeLayout.clientWidth);
  const resetBounds = await weightRow.getByRole("button", { name: /기본값 복원/ }).boundingBox();
  const formBounds = await page.locator(".settings-form").boundingBox();
  if (!resetBounds || !formBounds) throw new Error("Slider reset button and settings form must be visible");
  expect(resetBounds.x + resetBounds.width, "slider reset button must remain inside the visible settings form").toBeLessThanOrEqual(formBounds.x + formBounds.width + 1);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-exact-input-ko.png`), fullPage: true });

  const sliderBox = await weightSlider.boundingBox();
  if (!sliderBox) throw new Error("Normal weight slider must be visible");
  const y = sliderBox.y + sliderBox.height / 2;
  await page.mouse.move(sliderBox.x + sliderBox.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(sliderBox.x + sliderBox.width * 0.75, y, { steps: 12 });
  await page.mouse.up();

  const draggedValue = await exactWeight.inputValue();
  expect(Number(draggedValue)).toBeGreaterThan(0);
  await undo.click();
  await expect(exactWeight).toHaveValue("0");
  await redo.click();
  await expect(exactWeight).toHaveValue(draggedValue);

  await exactWeight.focus();
  await exactWeight.fill("6");
  await exactWeight.fill("64");
  await exactWeight.press("Tab");
  await expect(exactWeight).toHaveValue("64");
  await undo.click();
  await expect(exactWeight).toHaveValue(draggedValue);

  await exactWeight.focus();
  await exactWeight.fill("6");
  await exactWeight.fill("12");
  await undo.click();
  await expect(exactWeight).toHaveValue(draggedValue);

  await page.getByRole("button", { name: "고급·실험", exact: true }).click();
  const cacheRow = page.locator(".setting-row").filter({ hasText: "CacheMaxFaces" });
  const cacheValue = cacheRow.locator('input[type="number"]');
  await expect(cacheRow.locator('input[type="range"]')).toHaveCount(0);
  await expect(cacheValue).toHaveValue("64");
  await cacheValue.fill("128");
  await cacheValue.fill("256");
  await cacheValue.press("Enter");
  await undo.click();
  await expect(cacheValue).toHaveValue("64");
});

test("field revert restores the saved value while default restore and profile-wide reset use core defaults", async ({ page }) => {
  await page.goto("/?view=profiles&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "글자 모양", exact: true }).click();

  const weightRow = page.locator(".setting-row").filter({ hasText: "일반 글자 굵기" });
  const exactWeight = weightRow.locator('input[type="number"]');
  const revert = weightRow.getByRole("button", { name: /저장된 값으로 되돌리기/ });
  const restoreDefault = weightRow.getByRole("button", { name: /기본값 복원/ });

  // Clean profile: nothing to revert; the factory weight (16) differs from the
  // engine-default 0 the gallery profile starts from, so restore is available.
  await expect(revert).toBeDisabled();
  await expect(restoreDefault).toBeEnabled();

  await exactWeight.fill("12");
  await exactWeight.press("Enter");
  await page.getByRole("button", { name: "지금 저장", exact: true }).click();
  await expect(page.locator(".profile-message")).toContainText("지금 저장했습니다");
  await expect(revert).toBeDisabled();
  await expect(restoreDefault).toBeEnabled();

  await exactWeight.fill("30");
  await exactWeight.press("Enter");
  await expect(revert).toBeEnabled();
  await revert.click();
  await expect(exactWeight).toHaveValue("12");
  await expect(revert).toBeDisabled();

  await restoreDefault.click();
  await expect(exactWeight).toHaveValue("16");
  await expect(restoreDefault).toBeDisabled();
  await page.getByRole("button", { name: "되돌리기", exact: true }).click();
  await expect(exactWeight).toHaveValue("12");

  const gammaRow = page.locator(".setting-row").filter({ hasText: "감마 방식" });
  const gammaSelect = gammaRow.locator("select");
  await expect(gammaSelect).toHaveValue("-1");
  await gammaSelect.selectOption("2");
  await page.getByRole("button", { name: "기본값 초기화", exact: true }).click();
  await expect(exactWeight).toHaveValue("16");
  await expect(gammaSelect).toHaveValue("0");
  await page.getByRole("button", { name: "되돌리기", exact: true }).click();
  await expect(exactWeight).toHaveValue("12");
  await expect(gammaSelect).toHaveValue("2");

  await page.getByRole("button", { name: "변경 취소", exact: true }).click();
  await expect(exactWeight).toHaveValue("12");
  await expect(gammaSelect).toHaveValue("-1");
});

test("a rejected profile mutation requires an explicit snapshot recovery before save or apply", async ({ page }) => {
  await page.goto("/?view=profiles&gallery=1&lang=en&profile-fail-setting=normal_weight", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Glyph shape", exact: true }).click();

  const normalWeight = page.locator(".setting-row").filter({ hasText: "Normal weight" }).locator('input[type="number"]');
  const boldWeight = page.locator(".setting-row").filter({ hasText: "Bold weight" }).locator('input[type="number"]');
  const save = page.getByRole("button", { name: "Save now", exact: true });
  const apply = page.getByRole("button", { name: "Apply now", exact: true });

  await normalWeight.fill("12");
  await normalWeight.press("Tab");
  await expect(page.getByText("Gallery profile mutation failed.", { exact: true })).toBeVisible();
  await expect(save).toBeDisabled();
  await expect(apply).toBeDisabled();

  await boldWeight.fill("8");
  await boldWeight.press("Tab");
  await expect(save).toBeDisabled();
  await expect(apply).toBeDisabled();

  await page.getByRole("button", { name: "Discard changes", exact: true }).click();
  await expect(normalWeight).toHaveValue("0");
  await expect(boldWeight).toHaveValue("0");
  await expect(apply).toBeEnabled();
});

test("an unmounted profile preview ignores an in-flight completion", async ({ page }) => {
  await page.goto("/?view=profiles&gallery=1&lang=en&ci-smoke=1&preview-delay=1000", { waitUntil: "domcontentloaded" });
  await expect.poll(() => page.evaluate(() => Number(window.sessionStorage.getItem("gallery-preview-started") ?? "0"))).toBeGreaterThan(0);

  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await page.waitForTimeout(1200);

  await expect.poll(() => page.evaluate(() => window.sessionStorage.getItem("gallery-preview-crashes") ?? "0")).toBe("0");
  await expect.poll(() => page.evaluate(() => window.sessionStorage.getItem("gallery-profile-ready") ?? "0")).toBe("0");
});

test("settings files support import, save as, export, reveal, and apply without typing a path", async ({ page }) => {
  const failures: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") failures.push(`console: ${message.text()}`);
  });
  page.on("pageerror", (error) => failures.push(`pageerror: ${error.message}`));

  await page.goto("/?view=files&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await expect(page.getByRole("heading", { name: "기존 MacType 설정을 찾았습니다" })).toHaveCount(0);
  await expect(page.locator(".selected-file-summary")).toContainText("ini\\Default.ini");
  await expect(page.getByRole("textbox", { name: /경로|path/i })).toHaveCount(0);

  await page.getByRole("button", { name: "INI 파일 선택" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("Community.ini");
  await page.getByRole("textbox", { name: "복제 프로필 이름" }).fill("Gallery copy");
  await page.getByRole("button", { name: "다른 이름으로 저장" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("Gallery copy.ini");
  await page.getByRole("button", { name: "파일 위치 열기" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("파일 위치를 열었습니다");
  await page.getByRole("button", { name: "내보낼 위치 선택" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("내보냈습니다");
  await page.getByRole("button", { name: "실제 적용" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("시스템 프로필로 적용");

  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
  expect(horizontalOverflow, "settings-file controls must not have horizontal scrolling").toBe(false);
  expect(failures, failures.join("\n")).toEqual([]);
});

test("settings files use available width for profile paths", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "Desktop width proves the selector can use available space.");

  await page.goto("/?view=files&gallery=1&lang=en", { waitUntil: "networkidle" });
  const pretendardCard = page.locator(".profile-card").filter({ hasText: "Pretendard forever" });
  await pretendardCard.locator(".profile-card-select").click();
  await expect(pretendardCard).toHaveAttribute("data-selected", "true");

  const cardPaths = page.locator(".profile-card-select code");
  for (let index = 0; index < await cardPaths.count(); index += 1) {
    const cardMetrics = await cardPaths.nth(index).evaluate((element) => ({
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
    }));
    expect(cardMetrics.scrollWidth).toBeLessThanOrEqual(cardMetrics.clientWidth);
  }

  const displayedPath = page.locator(".selected-file-path code");
  const pathMetrics = await displayedPath.evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
  }));
  expect(pathMetrics.scrollWidth).toBeLessThanOrEqual(pathMetrics.clientWidth);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-settings-files-responsive-path-en.png`), fullPage: true });
});

test("diagnostics omit internal preview protocol details", async ({ page }) => {
  await page.goto("/?view=diagnostics&gallery=1&lang=en", { waitUntil: "networkidle" });
  await expect(page.getByText("Preview Helper", { exact: true })).toHaveCount(0);
  await expect(page.getByText("IPC protocol", { exact: true })).toHaveCount(0);
  await expect(page.getByText("MTPC v1", { exact: true })).toHaveCount(0);
});

test("writable profiles save to the original and apply by portable identity", async ({ page }, testInfo) => {
  await page.goto("/?view=profiles&gallery=1&lang=en", { waitUntil: "networkidle" });
  await expect(page.locator(".profile-editing")).toContainText("Editing:");
  await expect(page.locator(".profile-editing code")).toHaveText("ini\\Default.ini");

  const setting = page.locator(".setting-row select").first();
  const initial = await setting.inputValue();
  const alternate = await setting.locator("option").evaluateAll(
    (options, current) => options.map((option) => (option as HTMLOptionElement).value).find((value) => value !== current),
    initial,
  );
  if (!alternate) throw new Error("A writable profile setting needs an alternate gallery value");
  await setting.selectOption(alternate);

  const save = page.getByRole("button", { name: "Save now", exact: true });
  const apply = page.getByRole("button", { name: "Apply now", exact: true });
  await expect(save).toBeEnabled();
  await expect(apply).toBeDisabled();
  await save.click();
  await expect(page.locator(".profile-message")).toContainText("Saved Default.ini");
  await expect(apply).toBeEnabled();
  await apply.click();
  await expect(page.locator(".profile-message")).toContainText("Applied Default.ini");

  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-direct-save-apply-en.png`), fullPage: true });
});

test("read-only profiles require Save as before apply", async ({ page }, testInfo) => {
  await page.goto("/?view=profiles&gallery=1&lang=en&profile-read-only=1", { waitUntil: "networkidle" });
  await expect(page.locator(".profile-editing code")).toHaveText("ini\\Default.ini");
  await expect(page.getByText("The original file cannot be written.", { exact: false })).toBeVisible();

  const setting = page.locator(".setting-row select").first();
  const initial = await setting.inputValue();
  const alternate = await setting.locator("option").evaluateAll(
    (options, current) => options.map((option) => (option as HTMLOptionElement).value).find((value) => value !== current),
    initial,
  );
  if (!alternate) throw new Error("A read-only profile setting needs an alternate gallery value");
  await setting.selectOption(alternate);

  await expect(page.getByRole("button", { name: "Save now", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled();
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-read-only-save-as-required-en.png`), fullPage: true });
  await page.getByRole("button", { name: "Save as", exact: true }).click();
  await page.getByRole("textbox", { name: "New profile name" }).fill("Review copy");
  await page.locator(".profile-save-as").getByRole("button", { name: "Save as", exact: true }).click();

  await expect(page.locator(".profile-editing code")).toHaveText("Profiles\\Review copy.ini");
  await expect(page.locator(".profile-message")).toContainText("Saved as Profiles\\Review copy.ini");
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeEnabled();
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-read-only-save-as-en.png`), fullPage: true });
});

test("known legacy-selected profiles open directly without an import detour", async ({ page }, testInfo) => {
  await page.goto("/?view=files&gallery=1&lang=en&fresh=1&profile-runtime-missing=1", { waitUntil: "networkidle" });
  await expect(page.locator(".legacy-import-banner")).toHaveCount(0);
  await expect(page.locator(".selected-file-summary strong")).toHaveText("Editing");
  await expect(page.locator(".selected-file-summary code")).toHaveText("ini\\pretendard forever.ini");
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-direct-open-en.png`), fullPage: true });
});

test("external legacy-selected profiles require an explicit import", async ({ page }, testInfo) => {
  await page.goto("/?view=files&gallery=1&lang=en&legacy-profile=external", { waitUntil: "networkidle" });
  const banner = page.locator(".legacy-import-banner");
  await expect(banner).toContainText("Existing MacType settings found");
  await expect(banner).toContainText("C:\\Users\\Gallery\\Downloads\\External.ini");
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-external-import-required-en.png`), fullPage: true });
  await banner.getByRole("button", { name: "Import these settings" }).click();
  await expect(banner).toHaveCount(0);
  await expect(page.locator(".selected-file-summary code")).toHaveText("Profiles\\External.ini");
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("Imported External.ini");
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-external-import-en.png`), fullPage: true });
});

test("execution and new system service controls remain interactive", async ({ page }) => {
  const failures: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") failures.push(`console: ${message.text()}`);
  });
  page.on("pageerror", (error) => failures.push(`pageerror: ${error.message}`));

  await page.goto("/?view=execution&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await openServiceDetails(page);
  const autostart = page.getByRole("checkbox");
  await autostart.check();
  await expect(page.getByText("로그인할 때 트레이로 시작합니다.")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "실행 파일의 전체 경로" })).toHaveCount(0);
  await page.getByRole("button", { name: "실행 파일 선택" }).click();
  await expect(page.getByTitle("C:\\Windows\\System32\\notepad.exe", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "트레이에 등록" }).click();
  await expect(page.locator(".registered-launchers li code").filter({ hasText: "C:\\Windows\\System32\\notepad.exe" })).toBeVisible();
  await page.getByRole("button", { name: "등록 프로그램 실행" }).click();
  await expect(page.getByText(/등록 프로그램 1개를 MacType로 시작/)).toBeVisible();
  await page.getByRole("button", { name: "MacType로 실행" }).click();
  await expect(page.getByText(/MacLoader를 통해 프로세스 4242/)).toBeVisible();
  await expect(page.getByRole("heading", { name: "시스템 범위 모드" })).toBeVisible();

  await expect(page.getByText("MacType 시스템 적용 중", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "새 프로세스 적용 중지" }).click();
  await expect(page.getByText("MacType 시스템 적용 꺼짐", { exact: true })).toBeVisible();
  await expect(page.getByText("MacType 시스템 적용을 잠시 껐습니다.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "현재 프로필 적용" }).click();
  await expect(page.getByText("MacType 시스템 적용 중", { exact: true })).toBeVisible();
  await expect(page.getByText("현재 프로필을 시스템 범위에 적용했습니다.", { exact: true })).toBeVisible();

  await expect(page.locator('[data-service-backend="open-source"]')).toContainText("MacType Control Center 서비스");
  await expect(page.locator('[data-service-backend="legacy-mactray"]')).toHaveCount(0);

  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
  expect(horizontalOverflow, "execution controls must not have horizontal scrolling").toBe(false);
  expect(failures, failures.join("\n")).toEqual([]);
});

test("manual launch offers running processes first and file browsing second", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=ko&system-service=ready", { waitUntil: "networkidle" });
  const manualRow = page.locator('details.service-row[data-kind="manual"]');
  await manualRow.locator("summary").click();

  await expect(manualRow.getByText("실행 중인 프로세스", { exact: true })).toBeVisible();
  const rows = manualRow.locator(".process-picker-row");
  await expect(rows).toHaveCount(5);
  await expect(rows.nth(0)).toContainText("code.exe");
  await expect(rows.nth(0)).toContainText("Visual Studio Code");
  await expect(rows.nth(0)).toContainText("PID 5678");
  await expect(rows.nth(1)).toContainText("제목 없음 - 메모장");
  await expect(manualRow.getByText("목록에 없는 실행 파일 찾아보기", { exact: true })).toBeVisible();

  const register = manualRow.getByRole("button", { name: "트레이에 등록" });
  await expect(register).toBeDisabled();

  await manualRow.getByLabel("프로세스 필터").fill("note");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("notepad.exe");

  await manualRow.getByRole("radio", { name: /notepad\.exe/ }).check();
  await expect(manualRow.locator(".target-selection strong")).toHaveText("notepad.exe");
  await expect(manualRow.locator(".target-selection code")).toHaveText("C:\\Tools\\notepad.exe");
  await expect(register).toBeEnabled();

  expect(await overflowingElements(page)).toEqual([]);
});

test("a running legacy service is never claimed as verified system application", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=ko&system-service=ready&legacy=migration-available", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService).toBeVisible();
  await expect(openService).toContainText("MacType Control Center 서비스");
  await expect(openService).toContainText("준비 완료");
  await expect(page.getByText("MacType 시스템 적용 중", { exact: true })).toHaveCount(0);
  await expect(openService.locator('[data-state="running-unverified"]')).toBeVisible();

  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy).toBeVisible();
  await expect(legacy).toContainText("레거시 MacTray");
  await expect(legacy.getByRole("button", { name: "마이그레이션" })).toBeEnabled();
  await expect(legacy.getByRole("button", { name: "레거시 서비스 제거" })).toBeDisabled();

  await page.getByRole("button", { name: "새 프로세스 적용 중지" }).click();
  await expect(openService.locator('[data-state="legacy-service-migrate"]')).toBeVisible();
  await expect(openService).toContainText("레거시 MacTray 서비스를 먼저 정리해야 합니다");
  await expect(page.getByRole("button", { name: "현재 프로필 적용" })).toBeDisabled();
  await expect(legacy.getByRole("button", { name: "마이그레이션" })).toBeEnabled();

  expect(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth)).toBe(false);
});

test("a verified migration cannot be started again while the retired legacy service is stopped", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=ko&system-service=ready&legacy=migration-available&legacy-state=stopped&legacy-retired=1", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.locator('[data-state="active"]')).toBeVisible();
  await expect(openService).toContainText("MacType 시스템 적용 중");

  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy).toContainText("중지됨");
  await expect(legacy.getByRole("button", { name: "마이그레이션" })).toBeDisabled();
});

test("a retired stopped legacy service leaves new-service recovery available", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=ko&system-service=migration-available&legacy=migration-available&legacy-state=stopped&legacy-retired=1", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.getByRole("button", { name: "서비스 설치" })).toBeEnabled();
  await expect(openService).not.toContainText("레거시 MacTray 서비스를 먼저 정리해야 합니다");

  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy.getByRole("button", { name: "마이그레이션" })).toBeDisabled();
});

test("applying from Settings files and Execution converges on the same verified state", async ({ page }) => {
  await page.goto("/?view=files&gallery=1&lang=en&system-service=migration-available", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Apply to MacType" }).click();
  await expect(page.getByText(/Applied .* as the system-wide MacType profile/)).toBeVisible();

  await page.getByRole("button", { name: "Service" }).click();
  await openServiceDetails(page);
  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.locator('[data-state="active"]')).toBeVisible();
  await expect(openService).toContainText("MacType system-wide rendering active");
});

for (const entry of [
  { view: "files", button: "Apply to MacType" },
  { view: "profiles", button: "Apply now" },
] as const) {
  test(`${entry.view} hides internal profile-application details behind the diagnostics message`, async ({ page }) => {
    await page.goto(`/?view=${entry.view}&gallery=1&lang=en&service-fail=publish-profile`, { waitUntil: "networkidle" });
    await page.getByRole("button", { name: entry.button }).first().click();

    await expect(page.getByText("The operation failed. Check the diagnostics log for details.", { exact: true })).toBeVisible();
    await expect(page.getByText(/control-center-internal-operation-failed/)).toHaveCount(0);
  });
}

test("a foreign legacy MacType service blocks activation and offers no migration", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=foreign", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.locator('[data-state="legacy-service-migrate"]')).toBeVisible();
  await expect(openService).toContainText("A foreign legacy MacTray service was detected");
  await expect(openService).toContainText("does not match the verified MacTray service");
  await expect(openService).not.toContainText("Use Migrate below");
  await expect(openService.getByRole("button", { name: "Apply current profile" })).toBeDisabled();
  await expect(openService.getByRole("button", { name: "Install service" })).toBeDisabled();
  await expect(openService.getByRole("button", { name: "Start service" })).toBeDisabled();

  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy).toBeVisible();
  await expect(legacy.getByRole("button", { name: "Migrate" })).toBeDisabled();
  await expect(legacy.getByRole("button", { name: "Remove legacy service" })).toBeDisabled();
});

test("a verified legacy service funnels activation through Migrate until it is removed", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=migration-available", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.locator('[data-state="legacy-service-migrate"]')).toBeVisible();
  await expect(openService).toContainText("A legacy MacTray service must be resolved first");
  await expect(openService).toContainText("A verified legacy MacTray service is installed");
  await expect(openService).not.toContainText("foreign legacy MacTray service");
  await expect(openService).not.toContainText("status could not be verified");
  await expect(openService.getByRole("button", { name: "Apply current profile" })).toBeDisabled();
  await expect(openService.getByRole("button", { name: "Install service" })).toBeDisabled();
  await expect(openService.getByRole("button", { name: "Start service" })).toBeDisabled();

  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy).toContainText("Verified MacTray service");
  await expect(legacy.getByRole("button", { name: "Migrate" })).toBeEnabled();

  await legacy.getByRole("button", { name: "Migrate" }).click();
  await page.getByRole("dialog", { name: "Migrate legacy MacTray?" }).getByRole("button", { name: "Continue migration" }).click();
  await expect(page.getByText("Migration to the new service passed verification.", { exact: true })).toBeVisible();

  await expect(legacy).toContainText("Stopped");
  await expect(openService.getByRole("button", { name: "Stop applying to new processes" })).toBeEnabled();
  await expect(openService.getByRole("button", { name: "Install service" })).toBeDisabled();

  await legacy.getByRole("button", { name: "Remove legacy service" }).click();
  await expect(page.getByText("The legacy service was removed.", { exact: true })).toBeVisible();
  await expect(page.locator('[data-service-backend="legacy-mactray"]')).toHaveCount(0);
});

test("internal migration failures show only the localized diagnostics instruction", async ({ page }) => {
  for (const locale of [
    { lang: "en", title: "Migrate legacy MacTray?", continue: "Continue migration", message: "Migration failed. Check the diagnostics log for details." },
    { lang: "ko", title: "레거시 MacTray를 마이그레이션할까요?", continue: "마이그레이션 계속", message: "마이그레이션에 실패했습니다. 자세한 내용은 진단 로그를 확인하세요." },
  ]) {
    await page.goto(`/?view=execution&gallery=1&lang=${locale.lang}&system-service=migration-available&legacy=migration-available&service-fail=migrate-from-legacy`, { waitUntil: "networkidle" });
    await openServiceDetails(page);
    const legacy = page.locator('[data-service-backend="legacy-mactray"]');
    await legacy.getByRole("button", { name: locale.lang === "ko" ? "마이그레이션" : "Migrate" }).click();
    await page.getByRole("dialog", { name: locale.title }).getByRole("button", { name: locale.continue }).click();
    await expect(page.getByText(locale.message, { exact: true })).toBeVisible();
    await expect(page.getByText(/control-center-internal-operation-failed|broker exit code|strict Ready/)).toHaveCount(0);
  }
});

test("legacy migration explains the verified transaction before it can continue", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=migration-available", { waitUntil: "networkidle" });

  const migrationTrigger = page.locator("[data-service-summary]").getByRole("button", { name: "Migrate" });
  await migrationTrigger.click();

  const dialog = page.getByRole("dialog", { name: "Migrate legacy MacTray?" });
  const cancel = dialog.getByRole("button", { name: "Cancel" });
  const continueMigration = dialog.getByRole("button", { name: "Continue migration" });
  await expect(dialog).toBeVisible();
  await expect(cancel).toBeFocused();
  await expect(dialog).toContainText("AppInit and exact legacy service configuration");
  await expect(dialog).toContainText("current INI state");
  await expect(dialog).toContainText("when a profile file exists");
  await expect(dialog).toContainText("Stops the legacy service");
  await expect(dialog).toContainText("installs and starts the new service");
  await expect(dialog).toContainText("Ready health, matching profile digest, and x86 and x64 smoke checks");
  await expect(dialog).toContainText("rolls back");
  await expect(dialog).toContainText("does not remove the legacy service");

  await page.keyboard.press("Shift+Tab");
  await expect(continueMigration).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(migrationTrigger).toBeFocused();
  await expect(page.getByText("Migration to the new service passed verification.", { exact: true })).toHaveCount(0);

  await migrationTrigger.click();
  await cancel.click();
  await expect(migrationTrigger).toBeFocused();

  await migrationTrigger.click();
  await continueMigration.click();
  await expect(page.getByText("Migration to the new service passed verification.", { exact: true })).toBeVisible();
  await expect(migrationTrigger).toHaveCount(0);
});

test("system service path is read-only and can reveal its installed location", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=ko&system-service=ready", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  const servicePath = "C:\\Program Files\\MacType Control Center\\Service\\mactype-service.exe";
  await expect(openService.locator("code", { hasText: servicePath })).toBeVisible();
  await expect(openService.getByRole("textbox")).toHaveCount(0);

  const reveal = openService.getByRole("button", { name: "서비스 위치 열기" });
  const bounds = await reveal.boundingBox();
  if (!bounds) throw new Error("The reveal service location button must be visible");
  expect(bounds.width).toBeGreaterThanOrEqual(40);
  expect(bounds.height).toBeGreaterThanOrEqual(40);
  await reveal.click();
  await expect(page.getByText("서비스 파일 위치를 열었습니다.", { exact: true })).toBeVisible();
});

test("an absent service never exposes a binary path or location action", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService).toContainText("Not installed");
  await expect(openService.locator(".service-path")).toHaveCount(0);
  await expect(openService.getByRole("button", { name: "Open service location" })).toHaveCount(0);
});

test("the service page keeps its normal state to one summary and one action", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=ready", { waitUntil: "networkidle" });

  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Profile");
  await expect(summary).toContainText("Default.ini");
  await expect(summary).toContainText("Native service");
  await expect(summary).toContainText("Running");
  await expect(summary.getByRole("button", { name: "Stop" })).toBeEnabled();
  await expect(summary.getByRole("button")).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "System-wide modes" })).toBeVisible();
  await expect(page.locator('details.service-row[data-kind="system"]')).toContainText("Current installation · Running · Ready");
  await expect(page.getByRole("button", { name: "Remove service" })).toBeHidden();
  await expect(page.locator("details.service-row[open]")).toHaveCount(0);
});

test("small degraded states stay in Details while failed configuration is actionable", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=degraded", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Running");
  await expect(summary).not.toContainText("Degraded");
  await expect(summary.getByRole("button", { name: "Repair service" })).toHaveCount(0);
  await expect(page.getByText("Service running without verified system application", { exact: true })).toBeHidden();
  await openServiceDetails(page);
  await expect(page.locator('[data-service-backend="open-source"]')).toContainText("Degraded");

  await page.goto("/?view=execution&gallery=1&lang=en&system-service=failed", { waitUntil: "networkidle" });
  await expect(summary).toContainText("Service configuration needs repair.");
  await expect(summary.getByRole("button", { name: "Repair service" })).toBeEnabled();
});

test("outdated services upgrade while only failed current services repair", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=outdated", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Update required");
  await expect(summary.getByRole("button", { name: "Upgrade service" })).toBeEnabled();
  await openServiceDetails(page);
  const outdated = page.locator('[data-service-backend="open-source"]');
  await expect(outdated.getByRole("button", { name: "Upgrade service" })).toBeEnabled();
  await expect(outdated.getByRole("button", { name: "Repair service" })).toHaveCount(0);

  await page.goto("/?view=execution&gallery=1&lang=en&system-service=degraded", { waitUntil: "networkidle" });
  await openServiceDetails(page);
  const degraded = page.locator('[data-service-backend="open-source"]');
  await expect(degraded.getByRole("button", { name: "Repair service" })).toHaveCount(0);
  await expect(degraded.getByRole("button", { name: "Upgrade service" })).toHaveCount(0);

  await page.goto("/?view=execution&gallery=1&lang=en&system-service=failed", { waitUntil: "networkidle" });
  await openServiceDetails(page);
  const failed = page.locator('[data-service-backend="open-source"]');
  await expect(failed.getByRole("button", { name: "Repair service" })).toBeEnabled();
  await expect(failed.getByRole("button", { name: "Upgrade service" })).toHaveCount(0);
});

test("a running unverified service remains stoppable without claiming it is inactive", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=degraded", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.getByRole("button", { name: "Stop applying to new processes" })).toBeEnabled();
  await expect(openService).toContainText("Service running without verified system application");
  await expect(openService).toContainText("The service is running, but verified system-wide rendering cannot be confirmed. Stop remains available for safe recovery.");
  await expect(openService).not.toContainText("Reopen the target app in this state to compare rendering without the applied settings.");
  await openService.getByRole("button", { name: "Stop applying to new processes" }).click();
  await expect(page.getByText("MacType system application is temporarily off.", { exact: true })).toBeVisible();
});

test("a running profile mismatch remains stoppable and identifies the mismatched generation", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=profile-mismatch", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Service running with a different profile");
  await expect(summary).toContainText("The running generation does not match the profile expected by Control Center.");
  await expect(summary.getByRole("button", { name: "Stop" })).toBeEnabled();
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.getByRole("button", { name: "Stop applying to new processes" })).toBeEnabled();
  await expect(openService).toContainText("Service running with a different profile");
  await expect(openService).toContainText("The running generation does not match the profile expected by Control Center. Stop remains available; verified system application is not claimed.");
  await expect(openService).toContainText("Profile mismatch");
  await expect(openService).not.toContainText("or not yet verified");
});

test("AppInit conflict preserves the backend-authorized recovery stop", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=legacy-conflict&legacy=migration-available&raw-active=1", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Service running while AppInit conflicts");
  await expect(summary).toContainText("AppInit registry mode prevents a verified system state");
  await expect(summary.getByRole("button", { name: "Stop" })).toBeEnabled();
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.getByRole("button", { name: "Stop applying to new processes" })).toBeEnabled();
  await expect(openService).toContainText("Service running while AppInit conflicts");
  await expect(openService).toContainText("The new service is running, but AppInit registry mode prevents a verified system state. Stop remains available; other mutations stay blocked.");
  await expect(openService).not.toContainText("MacType system-wide rendering active");
  const statusRow = openService.locator(".detail-list > div").filter({ hasText: "New service status" });
  await expect(statusRow.locator(".warning")).toBeVisible();
  await expect(statusRow.locator(".success")).toHaveCount(0);
  for (const name of ["Install service", "Start service", "Remove service"]) {
    await expect(openService.getByRole("button", { name })).toBeDisabled();
  }
  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await expect(legacy.getByRole("button", { name: "Migrate" })).toBeDisabled();
  await expect(legacy.getByRole("button", { name: "Remove legacy service" })).toBeDisabled();
});

test("AppInit remains prominent when there is no safe automatic recovery action", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=legacy-conflict&service-runtime=stopped", { waitUntil: "networkidle" });

  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("AppInit registry mode is active, so service installation and startup are blocked.");
  await expect(summary.locator("[data-prominent-exception]")).toHaveAttribute("data-kind", "appinit-conflict");
  await expect(summary.getByRole("button")).toHaveCount(0);
});

test("trusted MacTray and autostart conflicts are resolved in the required order", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy-tray=trusted-current&legacy-startup=hkcu-run", { waitUntil: "networkidle" });

  const conflict = page.locator("[data-legacy-tray-conflict]");
  const summaryInstall = page.locator("[data-service-summary]").getByRole("button", { name: "Install service" });
  await expect(conflict).toContainText("Existing MacTray is running");
  await expect(conflict.getByRole("button", { name: "Exit MacTray" })).toBeEnabled();
  await expect(conflict.getByRole("button", { name: "Check again" })).toBeEnabled();
  await expect(conflict.getByRole("button", { name: "Disable MacTray autostart" })).toHaveCount(0);
  await expect(summaryInstall).toHaveCount(0);

  await conflict.getByRole("button", { name: "Exit MacTray" }).click();
  await expect(conflict).toContainText("MacTray autostart must be disabled");
  await expect(conflict.getByRole("button", { name: "Exit MacTray" })).toHaveCount(0);
  await expect(conflict.getByRole("button", { name: "Disable MacTray autostart" })).toBeEnabled();
  await expect(summaryInstall).toHaveCount(0);

  await conflict.getByRole("button", { name: "Disable MacTray autostart" }).click();
  await expect(page.locator("[data-legacy-tray-conflict]")).toHaveCount(0);
  await expect(summaryInstall).toBeEnabled();
});

for (const fixture of [
  ["trusted-other", "MacTray is running in another user session"],
  ["untrusted", "A same-named process could not be trusted"],
  ["unknown", "MacTray tray mode status is unavailable"],
] as const) {
  test(`${fixture[0]} MacTray state remains fail-closed without an exit action`, async ({ page }) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy-tray=${fixture[0]}`, { waitUntil: "networkidle" });

    const conflict = page.locator("[data-legacy-tray-conflict]");
    const summaryInstall = page.locator("[data-service-summary]").getByRole("button", { name: "Install service" });
    await expect(conflict).toContainText(fixture[1]);
    await expect(conflict.getByRole("button", { name: "Exit MacTray" })).toHaveCount(0);
    await expect(conflict.getByRole("button", { name: "Check again" })).toBeEnabled();
    await expect(summaryInstall).toHaveCount(0);
  });
}

for (const fixture of [
  ["hkcu-run", "MacTray autostart must be disabled", "Disable MacTray autostart"],
  ["untrusted", "A MacTray autostart entry could not be trusted", null],
  ["unknown", "MacTray autostart status is unavailable", null],
] as const) {
  test(`${fixture[0]} MacTray autostart state is prominent and fail-closed`, async ({ page }) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy-startup=${fixture[0]}`, { waitUntil: "networkidle" });

    const summary = page.locator("[data-service-summary]");
    const conflict = summary.locator("[data-legacy-tray-conflict]");
    await expect(conflict).toContainText(fixture[1]);
    await expect(conflict.getByRole("button", { name: "Check again" })).toBeEnabled();
    await expect(conflict.getByRole("button", { name: "Exit MacTray" })).toHaveCount(0);
    if (fixture[2]) await expect(conflict.getByRole("button", { name: fixture[2] })).toBeEnabled();
    else await expect(conflict.getByRole("button", { name: "Disable MacTray autostart" })).toHaveCount(0);
    await expect(summary.getByRole("button", { name: "Install service" })).toHaveCount(0);
  });
}

test("a running new service with a legacy tray conflict offers only the verified stop", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=ready&legacy-tray=trusted-current", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  const conflict = summary.locator("[data-legacy-tray-conflict]");
  await expect(conflict).toContainText("Existing MacTray is running");
  await expect(conflict.getByRole("button", { name: "Check again" })).toBeEnabled();
  await expect(conflict.getByRole("button", { name: "Exit MacTray" })).toBeEnabled();
  await expect(summary.getByRole("button", { name: "Stop" })).toHaveCount(0);
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  await expect(openService.locator('[data-state="running-legacy-tray-conflict"]')).toBeVisible();
  await expect(openService).toContainText("Service running while MacTray conflicts");
  await expect(openService).not.toContainText("MacType system-wide rendering active");
  await expect(openService.getByRole("button", { name: "Stop applying to new processes" })).toBeEnabled();
  for (const name of ["Install service", "Start service", "Repair service", "Upgrade service", "Remove service"]) {
    const button = openService.getByRole("button", { name });
    if (await button.count()) await expect(button).toBeDisabled();
  }
});

for (const legacyState of ["running", "stopped", "start-pending", "stop-pending", "paused", "unknown", "continue-pending", "pause-pending"] as const) {
  test(`legacy migration requires a stable ${legacyState} service`, async ({ page }, testInfo) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=migration-available&legacy-state=${legacyState}`, { waitUntil: "networkidle" });
    const summary = page.locator("[data-service-summary]");
    if (legacyState === "running" || legacyState === "stopped") {
      await expect(summary).toContainText("Legacy MacTray was detected.");
      await expect(summary.getByRole("button", { name: "Migrate" })).toBeEnabled();
    } else {
      await expect(summary).toContainText("A legacy MacTray service must be resolved first");
      await expect(summary.getByRole("button")).toHaveCount(0);
    }
    await openServiceDetails(page);

    const migrate = page.locator('[data-service-backend="legacy-mactray"]').getByRole("button", { name: "Migrate" });
    if (legacyState === "running" || legacyState === "stopped") await expect(migrate).toBeEnabled();
    else await expect(migrate).toBeDisabled();
    await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-execution-detail-legacy-${legacyState}-en.png`), fullPage: true });
  });
}

test("a foreign same-name service is prominent without exposing an unsafe action", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=foreign-service", { waitUntil: "networkidle" });

  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("A service with the same name has an unexpected configuration");
  await expect(summary.locator("[data-prominent-exception]")).toHaveAttribute("data-kind", "foreign-service");
  await expect(summary.getByRole("button")).toHaveCount(0);
  await expect(page.getByText("Manage the new service", { exact: true })).toBeHidden();
});

test("a foreign legacy service and pending removal cannot hide in Details", async ({ page }) => {
  const summary = page.locator("[data-service-summary]");

  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=foreign", { waitUntil: "networkidle" });
  await expect(summary).toContainText("A foreign legacy MacTray service was detected");
  await expect(summary.locator("[data-prominent-exception]")).toHaveAttribute("data-kind", "legacy-service-foreign");
  await expect(summary.getByRole("button")).toHaveCount(0);

  await page.goto("/?view=execution&gallery=1&lang=en&system-service=delete-pending", { waitUntil: "networkidle" });
  await expect(summary).toContainText("Removal pending");
  await expect(summary.locator("[data-prominent-exception]")).toHaveAttribute("data-kind", "removal-pending");
  await expect(summary.getByRole("button")).toHaveCount(0);
});

const legacyServiceIdentityCases = [
  {
    id: "owned",
    query: "legacy=migration-available",
    kind: "migration",
    title: "Legacy MacTray was detected.",
    description: "A verified legacy MacTray service is installed.",
    detailWarning: null,
  },
  {
    id: "foreign",
    query: "legacy=foreign",
    kind: "legacy-service-foreign",
    title: "A foreign legacy MacTray service was detected",
    description: "does not match the verified MacTray service",
    detailWarning: "does not match the verified MacTray service",
  },
  {
    id: "uncertain",
    query: "legacy=inaccessible",
    kind: "legacy-service-uncertain",
    title: "Legacy MacTray service status could not be verified",
    description: "could not read enough service information",
    detailWarning: "could not read enough service information",
  },
] as const;

for (const identity of legacyServiceIdentityCases) {
  test(`legacy service ${identity.id} identity has distinct copy`, async ({ page }, testInfo) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=migration-available&${identity.query}`, { waitUntil: "networkidle" });

    const summary = page.locator("[data-service-summary]");
    await expect(summary).toContainText(identity.title);
    await expect(summary).toContainText(identity.description);
    await expect(summary.locator("[data-prominent-exception]")).toHaveAttribute("data-kind", identity.kind);
    for (const other of legacyServiceIdentityCases.filter((candidate) => candidate.id !== identity.id)) {
      await expect(summary).not.toContainText(other.title);
    }

    await openServiceDetails(page);
    const openService = page.locator('[data-service-backend="open-source"]');
    await expect(openService).toContainText(identity.id === "owned" ? "A legacy MacTray service must be resolved first" : identity.title);
    await expect(openService).toContainText(identity.description);

    const legacy = page.locator('[data-service-backend="legacy-mactray"]');
    if (identity.detailWarning) await expect(legacy).toContainText(identity.detailWarning);
    else {
      await expect(legacy).not.toContainText("does not match the verified MacTray service");
      await expect(legacy).not.toContainText("could not read enough service information");
    }
    await page.screenshot({
      path: path.join(galleryRoot, `${testInfo.project.name}-execution-detail-legacy-identity-${identity.id}-en.png`),
      fullPage: true,
    });
  });
}

for (const fixture of [
  "ready",
  "degraded",
  "initializing",
  "unknown-health",
  "failed",
  "outdated",
  "profile-mismatch",
  "legacy-conflict",
  "migration-available",
  "foreign-service",
  "inaccessible-service",
  "delete-pending",
]) {
  test(`open service gallery renders ${fixture} without claiming SCM running is ready`, async ({ page }, testInfo) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=${fixture}`, { waitUntil: "networkidle" });
    await openServiceDetails(page);
    const openService = page.locator('[data-service-backend="open-source"]');
    await expect(openService).toBeVisible();
    if (fixture !== "ready" && fixture !== "legacy-conflict") {
      await expect(page.getByText("MacType system-wide rendering active", { exact: true })).toHaveCount(0);
    }
    if (fixture === "legacy-conflict") {
      const legacy = page.locator('[data-service-backend="legacy-mactray"]');
      await expect(legacy).toContainText("AppInit registry mode is active");
    }
    expect(await overflowingElements(page)).toEqual([]);
    await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-execution-${fixture}-en.png`), fullPage: true });
  });
}

for (const unstableState of ["start-pending", "stop-pending", "paused", "unknown"]) {
  test(`new service mutations stay disabled while ${unstableState}`, async ({ page }, testInfo) => {
    await page.goto(`/?view=execution&gallery=1&lang=en&system-service=ready&service-runtime=${unstableState}`, { waitUntil: "networkidle" });
    await openServiceDetails(page);

    const mutationButtons = page.locator('[data-service-backend="open-source"] .system-injection-action, [data-service-backend="open-source"] .service-actions button');
    await expect(mutationButtons).not.toHaveCount(0);
    for (const button of await mutationButtons.all()) await expect(button).toBeDisabled();
    await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-execution-detail-runtime-${unstableState}-en.png`), fullPage: true });
  });
}

test("a stopped new service with no alternative offers Start and Remove", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=ready&service-runtime=stopped", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Stopped");
  await expect(summary.getByRole("button", { name: "Start service" })).toBeEnabled();
  await expect(summary.getByRole("button", { name: "Remove service" })).toBeEnabled();
  await expect(summary.getByRole("button")).toHaveCount(2);
  await expect(summary.locator(".success")).toHaveCount(0);
  await expect(summary.locator(".warning")).toHaveCount(0);
  await expect(summary.locator(".neutral-status")).toHaveCount(1);
});

test("a deliberately stopped service reports neutrally instead of claiming degradation", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=ready&service-runtime=stopped", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  const statusRow = openService.locator(".detail-list > div").filter({ hasText: "New service status" });
  await expect(statusRow).toContainText("Current installation · Stopped");
  await expect(statusRow).not.toContainText("Not checked");
  await expect(statusRow.locator(".neutral-status")).toBeVisible();
  await expect(statusRow.locator(".warning")).toHaveCount(0);

  const profileRow = openService.locator(".detail-list > div").filter({ hasText: "Active profile generation" });
  await expect(profileRow).toContainText("Service is off");
  await expect(profileRow).not.toContainText("mismatch");
  await expect(profileRow.locator(".neutral-status")).toBeVisible();
  await expect(profileRow.locator(".warning")).toHaveCount(0);
});

test("a stopped service surfaces a persisted degradation as a last-run record", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=degraded&service-runtime=stopped", { waitUntil: "networkidle" });
  const summary = page.locator("[data-service-summary]");
  await expect(summary).toContainText("Stopped");
  await expect(summary.locator(".neutral-status")).toHaveCount(1);
  await openServiceDetails(page);

  const openService = page.locator('[data-service-backend="open-source"]');
  const statusRow = openService.locator(".detail-list > div").filter({ hasText: "New service status" });
  await expect(statusRow).toContainText("Degraded during the last run");
  const profileRow = openService.locator(".detail-list > div").filter({ hasText: "Active profile generation" });
  await expect(profileRow).toContainText("Service is off");
});

test("the primary service action disables immediately while its mutation is busy", async ({ page }) => {
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=ready&service-runtime=stopped&service-delay=750", { waitUntil: "networkidle" });

  const summary = page.locator("[data-service-summary]");
  const start = summary.getByRole("button", { name: "Start service" });
  await expect(start).toBeEnabled();
  await start.click();
  await expect(summary.getByRole("button", { name: "Working…" })).toBeDisabled();
  await expect(summary.getByRole("button", { name: "Stop" })).toBeEnabled();
});

test("service migration gallery remains usable at low window height", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "Low-height desktop behavior is width-specific");
  await page.setViewportSize({ width: 1280, height: 420 });
  await page.goto("/?view=execution&gallery=1&lang=en&system-service=migration-available&legacy=migration-available", { waitUntil: "networkidle" });
  await openServiceDetails(page);

  expect(await page.evaluate(() => document.documentElement.scrollHeight > document.documentElement.clientHeight)).toBe(true);
  expect(await overflowingElements(page)).toEqual([]);
  const legacy = page.locator('[data-service-backend="legacy-mactray"]');
  await legacy.scrollIntoViewIfNeeded();
  await expect(legacy.getByRole("button", { name: "Migrate" })).toBeEnabled();
  await page.screenshot({ path: path.join(galleryRoot, "desktop-execution-migration-low-height-en.png"), fullPage: true });
});

test("overview summarizes the active service and discloses at most five successful activities", async ({ page }) => {
  await page.goto("/?view=overview&gallery=1&lang=ko&system-service=ready", { waitUntil: "networkidle" });
  await expect(page.getByRole("heading", { name: "MacType가 실행 중입니다" })).toBeVisible();
  const summary = page.locator("[data-overview-service]");
  await expect(summary).toContainText("ini\\Default.ini");
  await expect(summary).toContainText("새 서비스");
  await expect(summary).toContainText("정상");
  await expect(summary.getByRole("button", { name: "서비스" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "설치 구성" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "다음 작업" })).toHaveCount(0);

  const activity = page.locator("[data-recent-activity]");
  await expect(activity).toContainText("프로필 Default.ini 적용을 마쳤습니다.");
  await expect(activity.locator("ol")).toHaveCount(0);
  await activity.getByRole("button", { name: "펼치기" }).click();
  await expect(activity.locator("ol li")).toHaveCount(3);
  await expect(activity).not.toContainText("migrate-from-legacy");
  await activity.getByRole("button", { name: "접기" }).click();
  await expect(activity.locator("ol")).toHaveCount(0);
});

test("overview offers a Service shortcut only when the service needs attention", async ({ page }) => {
  await page.goto("/?view=overview&gallery=1&lang=en&system-service=ready&service-runtime=stopped", { waitUntil: "networkidle" });
  await expect(page.locator("[data-overview-service]").getByRole("button", { name: "Service" })).toBeVisible();
  await page.goto("/?view=overview&gallery=1&lang=en&system-service=failed", { waitUntil: "networkidle" });
  await expect(page.locator("[data-overview-service]").getByRole("button", { name: "Service" })).toBeVisible();
});

test("diagnostics owns installation controls and always shows the localized event timeline", async ({ page }) => {
  await page.goto("/?view=diagnostics&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await expect(page.getByRole("heading", { name: "설치 구성" })).toBeVisible();
  await page.getByRole("button", { name: "설치 위치 다시 찾기" }).click();
  await expect(page.locator('[data-operation="relocate"]')).toBeVisible();
  await page.getByRole("button", { name: "다시 연결" }).click();
  await expect(page.locator('[data-operation="reconnect"]')).toBeVisible();
  await expect(page.getByRole("log")).toBeVisible();
  await expect(page.getByRole("log")).toContainText("프로필 Default.ini 적용을 마쳤습니다.");
  await expect(page.getByRole("log")).not.toContainText("operation=migrate-from-legacy");
  const actions = page.locator("[data-log-disclosure-actions]");
  await expect(actions.getByRole("button")).toHaveCount(1);
  await expect(actions.getByRole("button", { name: "로그 폴더 열기" })).toBeVisible();

  await page.getByRole("button", { name: "진단 파일 내보내기" }).click();
  await expect(page.locator('[data-operation="export"]')).toContainText("diagnostics-gallery.txt");
  await page.getByRole("button", { name: "진단 정보 복사" }).click();
  await expect(page.locator('[data-operation="copy"]')).toBeVisible();
  await page.getByRole("button", { name: "로그 폴더 열기" }).click();
  await expect(page.locator('[data-operation="folder"]')).toContainText("ControlCenter");
});

test("event view options hide summaries, persist, and collapse repeated failures", async ({ page }) => {
  await page.goto("/?view=diagnostics&gallery=1&lang=ko", { waitUntil: "networkidle" });
  const summaries = page.locator('.event-row[data-code="injection-summary"]');
  await expect(summaries).toHaveCount(4);
  await expect(page.getByTestId("event-timeline")).toContainText("최근 1분 동안");
  const hideSummaries = page.getByRole("switch", { name: "적용 요약 숨기기" });
  for (const option of ["hideInjectionSummary", "collapseRepeatedFailures", "hideRoutine"]) {
    await expect(page.locator(`.event-view-option[data-option="${option}"] .switch-control > span`)).toBeVisible();
  }
  await page.locator('.event-view-option[data-option="hideInjectionSummary"] .switch-control > span').click();
  await expect(summaries).toHaveCount(0);
  await page.reload({ waitUntil: "networkidle" });
  await expect(hideSummaries).toBeChecked();
  await expect(summaries).toHaveCount(0);
  await hideSummaries.uncheck();
  await expect(summaries).toHaveCount(4);
  const repeated = page.locator('.event-row[data-code="injection-failed"]').filter({ hasText: "vgtray.exe" });
  await expect(repeated).toHaveCount(3);
  const collapse = page.getByRole("switch", { name: "반복된 적용 실패 접기" });
  await collapse.check();
  await expect(repeated).toHaveCount(1);
  await expect(repeated.locator(".event-repeat")).toHaveText("3회 반복");
  await expect(page.locator('.event-row[data-code="injection-failed"]').filter({ hasText: "firefox.exe" })).toHaveCount(1);
  await collapse.uncheck();
  await expect(repeated).toHaveCount(3);
  const routine = page.locator('.event-row[data-code="app-started"], .event-row[data-code="preview-helper-connected"], .event-row[data-code="profile-verified"]');
  await expect(routine).toHaveCount(3);
  await page.getByRole("switch", { name: "앱 실행·미리보기 기록 숨기기" }).check();
  await expect(routine).toHaveCount(0);
  await page.evaluate(() => localStorage.removeItem("mactype-control-center.event-view"));
});

test("event log sources omit absent files but report existing unreadable files", async ({ page }) => {
  await page.goto("/?view=diagnostics&gallery=1&lang=ko&events-absent=1", { waitUntil: "networkidle" });
  await page.locator("details.event-source-disclosure > summary").click();
  await expect(page.locator(".event-sources > div")).toHaveCount(2);
  await expect(page.locator(".event-sources")).not.toContainText("서비스 설치");
  await page.goto("/?view=diagnostics&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.locator("details.event-source-disclosure > summary").click();
  await expect(page.locator(".event-sources > div")).toHaveCount(3);
  await page.goto("/?view=diagnostics&gallery=1&lang=ko&events-unreadable=1", { waitUntil: "networkidle" });
  await page.locator("details.event-source-disclosure > summary").click();
  await expect(page.locator(".event-sources > div").nth(2)).toContainText("읽을 수 없음");
});

test("diagnostics event titles localize activation reasons, broker failures, and panics", async ({ page }) => {
  await page.goto("/?view=diagnostics&gallery=1&lang=ko", { waitUntil: "networkidle" });
  const timeline = page.getByTestId("event-timeline");
  await expect(timeline).toContainText("firefox.exe에 적용하지 못했습니다 (모듈을 불러오지 못함).");
  await expect(timeline).toContainText("x64 앱에 MacType을 적용하지 못했습니다(렌더러 확인 스레드 실패).");
  await expect(timeline).toContainText("Control Center가 예기치 않게 중단되었습니다");
  await expect(timeline).not.toContainText("module-load-failed");
  await expect(timeline).not.toContainText("renderer-evidence-thread-failed");
});

test("language setting switches every supported locale and persists", async ({ page }, testInfo) => {
  await page.goto("/?view=overview&gallery=1&lang=ko", { waitUntil: "networkidle" });
  for (const locale of galleryLocales) {
    await page.getByTestId("language-picker-trigger").click();
    await page.locator(`[data-locale-option="${locale.id}"]`).click();
    await expect(page.locator("html")).toHaveAttribute("lang", locale.id);
    await expect(page.locator("html")).toHaveAttribute("dir", locale.direction);
    await expect(page.getByRole("heading", { level: 1, name: galleryViews[0].title[locale.id] })).toBeVisible();
  }

  await page.getByTestId("language-picker-trigger").click();
  await page.locator('[data-locale-option="en"]').click();
  await expect(page.getByRole("button", { name: "Dark theme" })).toBeVisible();

  await page.goto("/?view=overview&gallery=1", { waitUntil: "networkidle" });
  await expect(page.getByTestId("language-picker-trigger")).toHaveText("English");
  await expect(page.getByRole("heading", { level: 1, name: "Overview" })).toBeVisible();
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-language-en.png`), fullPage: true });
});

test("sidebar preferences stay at the bottom and yield to scrolling when height is tight", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-1280", "Desktop sidebar behavior is width-specific");
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/?view=overview&gallery=1&lang=ko", { waitUntil: "networkidle" });
  const sidebar = page.locator(".navigation");
  const preferences = page.locator(".navigation-preferences");

  const roomyGap = await page.evaluate(() => {
    const sidebarRect = document.querySelector<HTMLElement>(".navigation")!.getBoundingClientRect();
    const preferencesRect = document.querySelector<HTMLElement>(".navigation-preferences")!.getBoundingClientRect();
    return sidebarRect.bottom - preferencesRect.bottom;
  });
  expect(roomyGap).toBeCloseTo(16, 0);
  await page.screenshot({ path: path.join(galleryRoot, "desktop-sidebar-preferences-roomy.png"), fullPage: true });

  await page.setViewportSize({ width: 1280, height: 300 });
  const tightMetrics = await sidebar.evaluate((element) => {
    element.scrollTop = 0;
    const preferencesRect = element.querySelector<HTMLElement>(".navigation-preferences")!.getBoundingClientRect();
    return {
      overflows: element.scrollHeight > element.clientHeight,
      preferencesBelowFold: preferencesRect.bottom > element.getBoundingClientRect().bottom,
    };
  });
  expect(tightMetrics).toEqual({ overflows: true, preferencesBelowFold: true });

  await sidebar.evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
  await expect.poll(async () => {
    const sidebarBox = await sidebar.boundingBox();
    const preferencesBox = await preferences.boundingBox();
    return Math.round((sidebarBox?.y ?? 0) + (sidebarBox?.height ?? 0) - ((preferencesBox?.y ?? 0) + (preferencesBox?.height ?? 0)));
  }).toBe(16);
  await page.screenshot({ path: path.join(galleryRoot, "desktop-sidebar-preferences-tight.png") });
  await page.getByRole("button", { name: "어두운 테마" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByTestId("language-picker-trigger").click();
  await page.locator('[data-locale-option="en"]').click();
  await expect(page.getByRole("heading", { level: 1, name: "Overview" })).toBeVisible();
});

test("dark language menu and custom titlebar follow the application theme", async ({ page }, testInfo) => {
  await page.goto("/?view=overview&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await expect(page.getByRole("button", { name: "창 최소화" })).toBeVisible();
  await expect(page.getByRole("button", { name: "창 최대화 또는 복원" })).toBeVisible();
  await expect(page.getByRole("button", { name: "창 닫기" })).toBeVisible();

  await page.getByRole("button", { name: "어두운 테마" }).click();
  await page.getByTestId("language-picker-trigger").click();
  const menu = page.getByRole("listbox", { name: "표시 언어" });
  await expect(menu).toBeVisible();
  const themeColors = await page.evaluate(() => ({
    menu: getComputedStyle(document.querySelector<HTMLElement>(".language-menu")!).backgroundColor,
    titlebar: getComputedStyle(document.querySelector<HTMLElement>(".window-titlebar")!).backgroundColor,
  }));
  expect(themeColors).toEqual({ menu: "rgb(25, 32, 39)", titlebar: "rgb(25, 32, 39)" });
  expect(await menu.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-dark-language-titlebar.png`), fullPage: true });
});

test("theme setting persists across launches", async ({ page }) => {
  await page.goto("/?view=overview&gallery=1&lang=ko", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "어두운 테마" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await page.reload({ waitUntil: "networkidle" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.getByRole("button", { name: "밝은 테마" })).toBeVisible();

  await page.getByRole("button", { name: "밝은 테마" }).click();
  await page.reload({ waitUntil: "networkidle" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.getByRole("button", { name: "어두운 테마" })).toBeVisible();
});

test("settings files prefer the most recently worked profile", async ({ page }) => {
  const recent = "C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\profiles\\Recent.ini";
  await page.addInitScript(({ key, value }) => window.localStorage.setItem(key, value), {
    key: "mactype-control-center.recent-profile",
    value: recent,
  });
  await page.goto("/?view=files&gallery=1&lang=ko&fresh=1", { waitUntil: "networkidle" });
  await expect(page.locator('.profile-card[data-selected="true"] .profile-card-title strong')).toHaveText("Recent");
  await expect(page.locator(".selected-file-summary code")).toHaveText("Profiles\\Recent.ini");
});

test("settings files fall back to the applied profile", async ({ page }) => {
  await page.goto("/?view=files&gallery=1&lang=ko&fresh=1", { waitUntil: "networkidle" });
  await expect(page.locator('.profile-card[data-selected="true"] .profile-card-title strong')).toHaveText("Default");
  await expect(page.locator(".selected-file-summary code")).toHaveText("ini\\Default.ini");
});

test("settings files do not claim the already applied legacy profile is different", async ({ page }) => {
  await page.goto("/?view=files&gallery=1&lang=ko&legacy-applied=1", { waitUntil: "networkidle" });
  await expect(page.locator(".legacy-import-banner")).toHaveCount(0);
});

test("settings files present profile cards with thumbnails, apply ownership, and a tuner hand-off", async ({ page }, testInfo) => {
  await page.goto("/?view=files&gallery=1&lang=ko", { waitUntil: "networkidle" });

  const cards = page.locator(".profile-card");
  await expect(cards).toHaveCount(3);
  await expect(page.locator(".profile-card-thumb img")).toHaveCount(3);

  const appliedCard = page.locator('.profile-card[data-applied="true"]');
  await expect(appliedCard).toHaveCount(1);
  await expect(appliedCard.locator(".profile-card-title strong")).toHaveText("Default");
  await expect(appliedCard.locator(".profile-card-badge")).toHaveText("적용 중");

  const pretendardCard = cards.filter({ hasText: "Pretendard forever" });
  await pretendardCard.locator(".profile-card-select").click();
  await expect(pretendardCard).toHaveAttribute("data-selected", "true");
  await expect(page.locator(".selected-file-summary code")).toHaveText("ini\\pretendard forever.ini");

  const details = page.locator("details.file-details");
  await expect(details.locator("summary")).toContainText("파일 상세");
  await expect(details.locator("summary")).toContainText("UTF-8 · CRLF");
  await expect(details.locator(".detail-list")).toBeHidden();
  await details.locator("summary").click();
  await expect(details.locator(".detail-list")).toBeVisible();
  await expect(details).toContainText("문자 인코딩");

  await page.getByRole("button", { name: "실제 적용" }).click();
  await expect(page.locator('[data-operation="file-settings"]')).toContainText("시스템 프로필로 적용했습니다");
  await expect(appliedCard).toHaveCount(1);
  await expect(appliedCard.locator(".profile-card-title strong")).toHaveText("Pretendard forever");

  expect(await overflowingElements(page)).toEqual([]);
  await page.screenshot({ path: path.join(galleryRoot, `${testInfo.project.name}-profile-card-selector-ko.png`), fullPage: true });

  const editInTuner = page.getByRole("button", { name: "튜너에서 편집" });
  await expect(editInTuner).toHaveCount(3);
  await editInTuner.first().click();
  await expect(page.locator("body")).toHaveAttribute("data-view", "profiles");
  await expect(page.locator("body")).toHaveAttribute("data-profile-mode", "advanced");
});

