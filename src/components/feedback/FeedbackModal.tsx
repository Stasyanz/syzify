import { useEffect, useRef, useState } from "react";
import { X, Copy } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorText } from "../../lib/errors";
import { useFeedbackStore } from "../../stores/feedbackStore";
import { useToastStore } from "../../stores/toastStore";
import { Select } from "../ui/Select";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../../lib/contact";
import { copyText } from "../../lib/clipboard";
import { useRichT, useT } from "../../lib/i18n";

const APP_VERSION = __APP_VERSION__;
const MAX_MESSAGE_LENGTH = 1500;
const MIN_MESSAGE_LENGTH = 10;

type Category = "bug" | "feature";

/** The email the developer reads: its subject and the version line stay
 * English whatever the interface language — they sort the mailbox, the
 * user's own words are the message. */
export function buildMailtoUrl(
  to: string,
  category: Category,
  message: string,
): string {
  const subjectPrefix = category === "bug" ? "Bug Report" : "Feature Request";
  const subject = `[${subjectPrefix}] Feedback from Syzify`;
  const body = [message, "", `App version: ${APP_VERSION}`].join("\n");

  // mailto: query is a plain RFC 3986 URI, not application/x-www-form-urlencoded —
  // "+" is a literal character there, so URLSearchParams (which encodes space
  // as "+") corrupts subject/body in strict mail clients. encodeURIComponent
  // gives %20 for spaces instead. The recipient is a known-shape constant
  // ("@" is valid unencoded per RFC 6068), so it's left as-is: some handlers
  // don't decode it and would silently fail to address the email.
  return `mailto:${to}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}

export function FeedbackModal() {
  const t = useT();
  const rt = useRichT();
  const isOpen = useFeedbackStore((s) => s.isOpen);
  const close = useFeedbackStore((s) => s.close);
  const addToast = useToastStore((s) => s.addToast);

  const [category, setCategory] = useState<Category>("bug");
  const [message, setMessage] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  // Transient "can't type further" flash when the message hits the cap.
  const [limitHit, setLimitHit] = useState(false);
  const limitTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => () => clearTimeout(limitTimer.current), []);

  if (!isOpen) return null;

  function validate(): boolean {
    const errs: Record<string, string> = {};
    if (message.trim().length < MIN_MESSAGE_LENGTH) {
      errs.message = t("feedback.tooShort", { count: MIN_MESSAGE_LENGTH });
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }

  async function handleSend() {
    if (!validate()) return;

    const url = buildMailtoUrl(CONTACT_EMAIL, category, message.trim());
    try {
      await openUrl(url);
      addToast("info", t("feedback.opened"));
      close();
      setCategory("bug");
      setMessage("");
      setErrors({});
    } catch (err) {
      addToast("error", t("feedback.openFailed", { error: errorText(err) }));
    }
  }

  async function handleCopyAddress() {
    if (await copyText(CONTACT_EMAIL)) {
      addToast("success", t("feedback.copied"));
    } else {
      addToast("error", t("feedback.copyFailed"));
    }
  }

  function handleClose() {
    close();
    setErrors({});
  }

  return (
    // No backdrop-click close (app-wide modal policy): a stray click must
    // not discard a half-written message. Closing is explicit — X or Cancel.
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-md mx-4 p-6 space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">{t("feedback.title")}</h2>
          <button onClick={handleClose} className="text-faint hover:text-muted" aria-label={t("common.close")}>
            <X size={18} />
          </button>
        </div>

        {/* Category */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("feedback.category")}</label>
          <Select
            ariaLabel={t("feedback.category")}
            className="w-full"
            value={category}
            onChange={(v) => setCategory(v as Category)}
            options={[
              { value: "bug", label: t("feedback.bug") },
              { value: "feature", label: t("feedback.feature") },
            ]}
          />
        </div>

        {/* Message */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("feedback.message")}</label>
          <textarea
            value={message}
            onChange={(e) => {
              const v = e.target.value;
              if (v.length > MAX_MESSAGE_LENGTH) {
                setMessage(v.slice(0, MAX_MESSAGE_LENGTH));
                setLimitHit(true);
                clearTimeout(limitTimer.current);
                limitTimer.current = setTimeout(() => setLimitHit(false), 1500);
              } else {
                setMessage(v);
              }
            }}
            rows={4}
            maxLength={MAX_MESSAGE_LENGTH}
            placeholder={t("feedback.placeholder")}
            className={`w-full text-sm border rounded px-3 py-2 resize-y ${
              limitHit
                ? "border-red-500"
                : errors.message
                  ? "border-red-300"
                  : "border-border"
            }`}
          />
          {limitHit ? (
            <p role="status" className="text-xs text-red-500 mt-0.5">
              {t("feedback.max", { count: MAX_MESSAGE_LENGTH })}
            </p>
          ) : (
            errors.message && (
              <p className="text-xs text-red-500 mt-0.5">{errors.message}</p>
            )
          )}
        </div>

        <p className="text-xs text-faint">{t("feedback.attach")}</p>

        {/* Escape hatch for machines with no default mail client:
            Send opens nothing there, so show the address itself. Hidden
            entirely when CONTACT_EMAIL is unset — an empty address has
            nothing to show or copy. */}
        {CONTACT_EMAIL && (
          <p className="text-xs text-faint">
            {rt("feedback.noClient", { address: <span className="text-muted">{CONTACT_EMAIL}</span> })}{" "}
            <button
              type="button"
              onClick={handleCopyAddress}
              className="align-middle text-faint hover:text-muted"
              data-tip={t("feedback.copyAddress")}
              aria-label={t("feedback.copyAddress")}
            >
              <Copy size={12} />
            </button>
          </p>
        )}

        <p className="text-xs text-faint">
          {rt("feedback.preferGithub", {
            link: (
              <a
                href={GITHUB_ISSUES_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="text-muted hover:underline"
              >
                {t("feedback.openIssue")}
              </a>
            ),
          })}
        </p>

        {/* Actions */}
        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={handleClose}
            className="text-sm px-4 py-2 text-muted hover:text-ink"
          >
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={handleSend}
            className="text-sm px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent-2"
          >
            {t("feedback.send")}
          </button>
        </div>
      </div>
    </div>
  );
}
