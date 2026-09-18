export function askText(title: string, initial: string): Promise<string | null> {
  return new Promise((resolve) => {
    const dialog = document.createElement("dialog");
    const form = document.createElement("form");
    form.method = "dialog";
    const heading = document.createElement("h2");
    heading.textContent = title;
    const input = document.createElement("textarea");
    input.rows = 4;
    input.cols = 52;
    input.value = initial;
    const ok = document.createElement("button");
    ok.value = "ok";
    ok.textContent = "确定";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "取消";
    cancel.onclick = () => dialog.close();
    form.append(heading, input, ok, cancel);
    dialog.append(form);
    dialog.addEventListener("close", () => {
      resolve(dialog.returnValue === "ok" ? input.value : null);
      dialog.remove();
    }, { once: true });
    document.body.append(dialog);
    dialog.showModal();
    input.focus();
    input.select();
  });
}
