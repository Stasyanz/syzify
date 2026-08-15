import { useEffect, useRef, useState } from "react";
import { X, Copy } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useFeedbackStore } from "../../stores/feedbackStore";
import { useToastStore } from "../../stores/toastStore";
import { Select } from "../ui/Select";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../../lib/contact";
import { copyText } from "../../lib/clipboard";

const APP_VERSION = __APP_VERSION__;
const MAX_MESSAGE_LENGTH = 1500;

type Category = "bug" | "feature";

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
    if (message.trim().length < 10) {
      errs.message = "Message must be at least 10 characters";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }

  async function handleSend() {
    if (!validate()) return;

    const url = buildMailtoUrl(CONTACT_EMAIL, category, message.trim());
    try {
      await openUrl(url);
      addToast("info", "Email client opened — please send the email");
      close();
      setCategory("bug");
      setMessage("");
      setErrors({});
    } catch (err) {
      addToast("error", `Failed to open email client: ${err}`);
    }
  }

  async function handleCopyAddress() {
    if (await copyText(CONTACT_EMAIL)) {
      addToast("success", "Address copied");
    } else {
      addToast("error", "Copy failed — select the address manually");
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
          <h2 className="text-lg font-semibold">Send Feedback</h2>
          <button onClick={handleClose} className="text-faint hover:text-muted">
            <X size={18} />
          </button>
        </div>

        {/* Category */}
        <div>
          <label className="text-xs text-muted block mb-1">Category</label>
          <Select
            ariaLabel="Category"
            className="w-full"
            value={category}
            onChange={(v) => setCategory(v as Category)}
            options={[
              { value: "bug", label: "Bug Report" },
              { value: "feature", label: "Feature Request" },
            ]}
          />
        </div>

        {/* Message */}
        <div>
          <label className="text-xs text-muted block mb-1">Message</label>
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
            placeholder="Describe the issue or suggestion..."
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
              Max {MAX_MESSAGE_LENGTH} characters
            </p>
          ) : (
            errors.message && (
              <p className="text-xs text-red-500 mt-0.5">{errors.message}</p>
            )
          )}
        </div>

        <p className="text-xs text-faint">
          You can attach screenshots directly in your email client
        </p>

        {/* Escape hatch for machines with no default mail client:
            Send opens nothing there, so show the address itself. Hidden
            entirely when CONTACT_EMAIL is unset — an empty address has
            nothing to show or copy. */}
        {CONTACT_EMAIL && (
          <p className="text-xs text-faint">
            No email client? Write to{" "}
            <span className="text-muted">{CONTACT_EMAIL}</span>{" "}
            <button
              type="button"
              onClick={handleCopyAddress}
              className="align-middle text-faint hover:text-muted"
              data-tip="Copy address"
              aria-label="Copy address"
            >
              <Copy size={12} />
            </button>
          </p>
        )}

        <p className="text-xs text-faint">
          Prefer GitHub?{" "}
          <a
            href={GITHUB_ISSUES_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="text-muted hover:underline"
          >
            Open an issue
          </a>
        </p>

        {/* Actions */}
        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={handleClose}
            className="text-sm px-4 py-2 text-muted hover:text-ink"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSend}
            className="text-sm px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent-2"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
