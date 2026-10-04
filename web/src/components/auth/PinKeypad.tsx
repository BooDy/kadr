import { useState, useEffect, useCallback, type FC } from 'react';
import { Delete, Lock } from 'lucide-react';
import { api } from '../../api/client';
import type { User } from '../../types';

export interface PinKeypadProps {
  user?: User;
  title?: string;
  subtitle?: string;
  onSubmitPin?: (pin: string) => Promise<void>;
  onSuccess: (token: string, user: User) => void;
  onCancel?: () => void;
}

export const PinKeypad: FC<PinKeypadProps> = ({
  user,
  title,
  subtitle,
  onSubmitPin,
  onSuccess,
  onCancel,
}) => {
  const [pin, setPin] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isShaking, setIsShaking] = useState(false);

  const submitPin = useCallback(
    async (pinToSubmit: string) => {
      setIsLoading(true);
      setError(null);

      try {
        if (onSubmitPin) {
          await onSubmitPin(pinToSubmit);
          onSuccess('', user ?? { id: '', username: '', role: 'standard' });
        } else if (user) {
          const res = await api.loginWithPin(pinToSubmit, user.id);
          onSuccess(res.token, res.user);
        }
      } catch (err: unknown) {
        setIsShaking(true);
        const msg =
          err instanceof Error
            ? err.message
            : 'Invalid PIN. Please try again.';
        setError(msg);
        setPin('');
        setTimeout(() => {
          setIsShaking(false);
        }, 500);
      } finally {
        setIsLoading(false);
      }
    },
    [user, onSubmitPin, onSuccess]
  );

  const handleDigit = useCallback(
    (digit: string) => {
      if (isLoading || pin.length >= 4) return;
      if (error) setError(null);

      const nextPin = pin + digit;
      setPin(nextPin);

      if (nextPin.length === 4) {
        submitPin(nextPin);
      }
    },
    [isLoading, pin, error, submitPin]
  );

  const handleBackspace = useCallback(() => {
    if (isLoading || pin.length === 0) return;
    if (error) setError(null);
    setPin((prev) => prev.slice(0, -1));
  }, [isLoading, pin.length, error]);

  // Physical keyboard support
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (isLoading) return;

      if (e.key >= '0' && e.key <= '9') {
        e.preventDefault();
        handleDigit(e.key);
      } else if (e.key === 'Backspace') {
        e.preventDefault();
        handleBackspace();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onCancel?.();
      } else if (e.key === 'Enter') {
        if (pin.length === 4) {
          e.preventDefault();
          submitPin(pin);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isLoading, handleDigit, handleBackspace, onCancel, pin, submitPin]);

  return (
    <div
      data-testid="pin-keypad-container"
      className={`bg-panel border border-border-subtle rounded-2xl p-6 sm:p-8 max-w-sm w-full mx-auto flex flex-col items-center select-none shadow-2xl ${
        isShaking ? 'animate-shake' : ''
      }`}
    >
      {/* Header (Custom Title or User Header) */}
      <div className="flex flex-col items-center text-center mb-6">
        {title ? (
          <>
            <div className="h-16 w-16 rounded-full bg-gradient-to-tr from-accent to-highlight p-0.5 shadow-lg shadow-black/40 mb-3">
              <div className="h-full w-full rounded-full bg-canvas flex items-center justify-center">
                <Lock className="h-7 w-7 text-highlight" />
              </div>
            </div>
            <h3 className="text-xl font-bold text-text-main tracking-tight">
              {title}
            </h3>
            <p className="text-xs text-muted mt-1 flex items-center gap-1.5">
              <Lock className="h-3 w-3 text-muted" />
              {subtitle || 'Enter 4-digit PIN'}
            </p>
          </>
        ) : user ? (
          <>
            <div className="h-16 w-16 rounded-full bg-gradient-to-tr from-accent to-highlight p-0.5 shadow-lg shadow-black/40 mb-3">
              <div className="h-full w-full rounded-full bg-canvas flex items-center justify-center">
                <span className="text-2xl font-bold uppercase tracking-wider text-accent">
                  {user.username.slice(0, 2) || 'U'}
                </span>
              </div>
            </div>
            <h3 className="text-xl font-bold text-text-main tracking-tight flex items-center gap-2">
              <span>{user.username}</span>
              {user.role === 'admin' && (
                <span className="rounded-full bg-cta px-2 py-0.5 text-[10px] font-semibold text-white">
                  ADMIN
                </span>
              )}
            </h3>
            <p className="text-xs text-muted mt-1 flex items-center gap-1.5">
              <Lock className="h-3 w-3 text-muted" />
              {subtitle || 'Enter your 4-digit PIN'}
            </p>
          </>
        ) : null}
      </div>

      {/* 4-digit PIN Indicator Dots */}
      <div
        className="flex items-center justify-center gap-4 mb-6"
        aria-label="PIN Entry Status"
      >
        {[0, 1, 2, 3].map((index) => {
          const isFilled = index < pin.length;
          return (
            <div
              key={index}
              data-testid={`pin-dot-${index}`}
              data-filled={isFilled ? 'true' : 'false'}
              aria-label={`Digit ${index + 1} ${isFilled ? 'filled' : 'empty'}`}
              className={`h-4 w-4 rounded-full transition-all duration-200 ${
                isFilled
                  ? 'bg-highlight scale-110 shadow-md shadow-highlight/30'
                  : 'bg-muted/40 border border-muted'
              }`}
            />
          );
        })}
      </div>

      {/* Error Message */}
      <div className="h-6 mb-4 flex items-center justify-center">
        {error && (
          <p
            role="alert"
            className="text-xs font-semibold text-cta tracking-wide transition-opacity"
          >
            {error}
          </p>
        )}
      </div>

      {/* Numeric Keypad Grid */}
      <div className="grid grid-cols-3 gap-3 place-items-center w-full max-w-[240px]">
        {['1', '2', '3', '4', '5', '6', '7', '8', '9'].map((digit) => (
          <button
            key={digit}
            type="button"
            disabled={isLoading}
            onClick={() => handleDigit(digit)}
            aria-label={digit}
            className="bg-canvas/60 hover:bg-canvas text-text-main hover:text-white border border-border-subtle rounded-xl text-2xl font-bold h-14 w-14 transition-all duration-150 active:scale-95 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none disabled:opacity-50 disabled:pointer-events-none flex items-center justify-center"
          >
            {digit}
          </button>
        ))}

        {/* Row 4: Cancel, 0, Backspace */}
        <button
          type="button"
          disabled={isLoading}
          onClick={() => onCancel?.()}
          aria-label="Cancel"
          className="h-14 w-14 rounded-xl text-muted hover:text-text-main hover:bg-canvas/40 border border-border-subtle text-[11px] font-semibold uppercase tracking-wider transition-all duration-150 active:scale-95 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none disabled:opacity-50 disabled:pointer-events-none flex items-center justify-center"
        >
          Cancel
        </button>

        <button
          type="button"
          disabled={isLoading}
          onClick={() => handleDigit('0')}
          aria-label="0"
          className="bg-canvas/60 hover:bg-canvas text-text-main hover:text-white border border-border-subtle rounded-xl text-2xl font-bold h-14 w-14 transition-all duration-150 active:scale-95 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none disabled:opacity-50 disabled:pointer-events-none flex items-center justify-center"
        >
          0
        </button>

        <button
          type="button"
          disabled={isLoading || pin.length === 0}
          onClick={handleBackspace}
          aria-label="Backspace"
          title="Backspace"
          className="h-14 w-14 rounded-xl text-muted hover:text-text-main hover:bg-canvas/40 border border-border-subtle transition-all duration-150 active:scale-95 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none disabled:opacity-30 disabled:pointer-events-none flex items-center justify-center"
        >
          <Delete className="h-5 w-5" />
        </button>
      </div>
    </div>
  );
};

export default PinKeypad;
