// Mirrors the installer's safe_text rules for user-entered help messages.
export function safeHelpText(input: string, username = ""): string {
  const partly = input
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "")
    .replace(/-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----[\s\S]*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|$)/g, "<PRIVATE_KEY>")
    .replace(/(?:^[ \t]*[A-Za-z0-9+/=_-]{20,}[ \t]*\r?\n)+[ \t]*-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----/gm, "<PRIVATE_KEY>")
    .replace(/[A-Za-z0-9._%+-]{1,64}@[A-Za-z0-9-]{1,63}(?:\.[A-Za-z0-9-]{1,63})*\.[A-Za-z]{2,63}/g, "<EMAIL>")
    .replace(/\bBearer\s+[A-Za-z0-9._~+/=-]+/gi, "<TOKEN>")
    .replace(/sk-[A-Za-z0-9_-]{8,}|(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]{20,}|AIza[0-9A-Za-z_-]{35}|xox[abprs]-[0-9A-Za-z-]{10,}|npm_[A-Za-z0-9]{36}|(?:AKIA|ASIA)[0-9A-Z]{16}|eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}/g, "<TOKEN>")
    .replace(/((?:[A-Za-z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|PASSWD))["']?\s*[=:]\s*["']?)[^\s"',;}]+/gi, "$1<SECRET>")
    .replace(/\b[A-Za-z0-9_-]{20,}#[A-Za-z0-9_-]{8,}\b/g, "<LOGIN_CODE>")
    .replace(/(Paste code here if prompted\s*>)[^\n]*/gi, "$1 <LOGIN_CODE>");
  const userPattern = username ? new RegExp(`(^|[^\\w])${username.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?=$|[^\\w])`, "gi") : null;
  return (userPattern ? partly.replace(userPattern, "$1<USER>") : partly)
    .replace(/[A-Za-z]:[\\/]+Users[\\/]+[^\\/\n"'|:*?]{1,64}?(?=[\\/])/gi, "C:\\Users\\<USER>")
    .replace(/[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s]+/gi, "C:\\Users\\<USER>")
    .replace(/\/(?:Users|home)\/[^/\n"'|:*?]{1,64}?(?=\/)/g, "/Users/<USER>")
    .replace(/\/(?:Users|home)\/[^/\s]+/g, "/Users/<USER>")
    .replace(/((?:USERNAME\s*=|whoami\s*:)\s*)[^\n]*/gi, "$1<USER>")
    .replace(/[\t\r\n]+/g, " ")
    .replace(/ +/g, " ")
    .trim()
    .slice(0, 500);
}
