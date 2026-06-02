import { createContext } from 'react'

export interface PrivacyContextValue {
  privacyMode: boolean;
  setPrivacyMode: (enabled: boolean) => Promise<void>;
  maskEmail: (email: string) => string;
  maskNickname: (name: string) => string;
}

export const PrivacyContext = createContext<PrivacyContextValue | null>(null)
