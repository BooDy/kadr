import { useState, useEffect, useCallback, type FC } from 'react';
import { Delete, Lock } from 'lucide-react';
import { api } from '../../api/client';
import type { User } from '../../types';

export interface PinKeypadProps {
  user: User;
  onSuccess: (token: string, user: User) => void;
  onCancel?: () => void;
}

export const PinKeypad: FC<PinKeypadProps> = ({ user, onSuccess, onCancel }) => {
  const [pin, setPin] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isShaking, setIsShaking] = useState(false);

  const submitPin = useCallback(
    async (pinToSubmit: string) => {
      setIsLoading(true);
      setError(null);

      try {
        const res = await api.loginWithPin(pinToSubmit, user.id);
        onSuccess(res.token, res.user);
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
    [user.id, onSuccess]
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
      className={`w-full max-w-sm mx-auto flex flex-col items-center select-none ${
        isShaking ? 'animate-shake' : ''
      }`}
    >
      {/* User Header */}
      <div className="flex flex-col items-center text-center mb-6">
        <div className="h-20 w-20 rounded-full bg-gradient-to-tr from-rose-500 via-rose-600 to-amber-500 p-0.5 shadow-xl shadow-rose-950/40 mb-3">
          <div className="h-full w-full rounded-full bg-zinc-950 flex items-center justify-center">
            <span className="text-2xl font-bold uppercase tracking-wider text-rose-400">
              {user.username.slice(0, 2) || 'U'}
            </span>
          </div>
        </div>
        <h3 className="text-xl font-bold text-white tracking-tight flex items-center gap-2">
          <span>{user.username}</span>
          {user.role === 'admin' && (
            <span className="rounded bg-rose-500/20 px-2 py-0.5 text-[10px] font-semibold text-rose-400 border border-rose-500/30">
              ADMIN
            </span>
          )}
        </h3>
        <p className="text-xs text-zinc-400 mt-1 flex items-center gap-1.5">
          <Lock className="h-3 w-3 text-zinc-500" />
          Enter your 4-digit PIN
        </p>
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
                  ? 'bg-rose-500 scale-110 shadow-lg shadow-rose-500/50 border border-rose-400'
                  : 'bg-zinc-800/80 border-2 border-zinc-700/80'
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
            className="text-xs font-semibold text-rose-400 tracking-wide transition-opacity"
          >
            {error}
          </p>
        )}
      </div>

      {/* Numeric Keypad Grid */}
      <div className="grid grid-cols-3 gap-3 w-full max-w-[280px]">
        {['1', '2', '3', '4', '5', '6', '7', '8', '9'].map((digit) => (
          <button
            key={digit}
            type="button"
            disabled={isLoading}
            onClick={() => handleDigit(digit)}
            aria-label={digit}
            className="h-16 rounded-2xl bg-zinc-900/90 border border-zinc-800/80 text-xl font-bold text-white shadow-md hover:bg-zinc-800 hover:border-zinc-700 active:scale-95 active:bg-zinc-700/80 disabled:opacity-50 disabled:pointer-events-none transition-all duration-150 flex items-center justify-center"
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
          className="h-16 rounded-2xl bg-zinc-900/40 border border-zinc-800/50 text-xs font-semibold uppercase tracking-wider text-zinc-400 hover:text-white hover:bg-zinc-800/60 active:scale-95 disabled:opacity-50 disabled:pointer-events-none transition-all duration-150 flex items-center justify-center"
        >
          Cancel
        </button>

        <button
          type="button"
          disabled={isLoading}
          onClick={() => handleDigit('0')}
          aria-label="0"
          className="h-16 rounded-2xl bg-zinc-900/90 border border-zinc-800/80 text-xl font-bold text-white shadow-md hover:bg-zinc-800 hover:border-zinc-700 active:scale-95 active:bg-zinc-700/80 disabled:opacity-50 disabled:pointer-events-none transition-all duration-150 flex items-center justify-center"
        >
          0
        </button>

        <button
          type="button"
          disabled={isLoading || pin.length === 0}
          onClick={handleBackspace}
          aria-label="Backspace"
          title="Backspace"
          className="h-16 rounded-2xl bg-zinc-900/40 border border-zinc-800/50 text-zinc-400 hover:text-rose-400 hover:bg-zinc-800/60 active:scale-95 disabled:opacity-30 disabled:pointer-events-none transition-all duration-150 flex items-center justify-center"
        >
          <Delete className="h-6 w-6" />
        </button>
      </div>
    </div>
  );
};

export default PinKeypad;
