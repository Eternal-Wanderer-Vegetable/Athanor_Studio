import { DEFAULT_PAGE_THEME, type PageTheme, normalizePageTheme } from "../app/theme";

function field(label: string, control: HTMLElement): HTMLLabelElement {
  const wrapper = document.createElement("label");
  wrapper.textContent = label;
  wrapper.append(control);
  return wrapper;
}

function select(values: string[], current: string): HTMLSelectElement {
  const el = document.createElement("select");
  for (const value of values) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = value;
    option.selected = value === current;
    el.append(option);
  }
  return el;
}

function numberInput(value: number): HTMLInputElement {
  const el = document.createElement("input");
  el.type = "number";
  el.min = "5";
  el.max = "60";
  el.step = "0.5";
  el.value = String(value);
  return el;
}

/** Open the native page setup dialog and resolve only after the user closes it. */
export function askPageSettings(initial: PageTheme = DEFAULT_PAGE_THEME): Promise<PageTheme | null> {
  const current = normalizePageTheme(initial);
  const dialog = document.createElement("dialog");
  dialog.className = "page-settings-dialog";
  const form = document.createElement("form");
  form.method = "dialog";
  const title = document.createElement("h2");
  title.textContent = "页面设置";
  form.append(title);

  const size = select(["A4", "Letter"], current.pageSize);
  const orientation = select(["portrait", "landscape"], current.orientation);
  const top = numberInput(current.margins.top);
  const right = numberInput(current.margins.right);
  const bottom = numberInput(current.margins.bottom);
  const left = numberInput(current.margins.left);
  const header = document.createElement("input");
  header.value = current.header;
  const footer = document.createElement("input");
  footer.value = current.footer;
  form.append(
    field("纸张", size),
    field("方向", orientation),
    field("上边距（mm）", top),
    field("右边距（mm）", right),
    field("下边距（mm）", bottom),
    field("左边距（mm）", left),
    field("页眉", header),
    field("页脚", footer),
  );
  const actions = document.createElement("div");
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "取消";
  const ok = document.createElement("button");
  ok.type = "submit";
  ok.textContent = "应用";
  actions.append(cancel, ok);
  form.append(actions);
  dialog.append(form);
  document.body.append(dialog);

  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: PageTheme | null) => {
      if (settled) return;
      settled = true;
      dialog.remove();
      resolve(value);
    };
    cancel.addEventListener("click", () => {
      dialog.close("cancel");
      finish(null);
    });
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      dialog.close("ok");
      finish(normalizePageTheme({
        ...current,
        pageSize: size.value,
        orientation: orientation.value,
        margins: {
          top: Number(top.value), right: Number(right.value),
          bottom: Number(bottom.value), left: Number(left.value),
        },
        header: header.value,
        footer: footer.value,
      }));
    });
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      dialog.close("cancel");
      finish(null);
    });
    dialog.addEventListener("close", () => finish(null));
    dialog.showModal();
  });
}
