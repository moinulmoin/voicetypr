import {
  hasSttApiKeySoniox,
  removeSttApiKeySoniox,
  saveSttApiKeySoniox,
} from '@/utils/keyring';

export interface CloudProviderDefinition {
  id: string;
  engine: 'soniox';
  modelName: string;
  displayName: string;
  description: string;
  providerName: string;
  addKey: (key: string) => Promise<void>;
  removeKey: () => Promise<void>;
  hasKey: () => Promise<boolean>;
  docsUrl?: string;
  setupCta?: string;
}

const sonioxProvider: CloudProviderDefinition = {
  id: 'soniox',
  engine: 'soniox',
  modelName: 'soniox',
  displayName: 'Soniox (Cloud)',
  description: 'High-accuracy streaming transcription via Soniox cloud APIs',
  providerName: 'Soniox',
  addKey: async (key: string) => {
    await saveSttApiKeySoniox(key.trim());
  },
  removeKey: async () => {
    await removeSttApiKeySoniox();
  },
  hasKey: async () => hasSttApiKeySoniox(),
  docsUrl: 'https://soniox.com/docs/stt/get-started',
  setupCta: 'Add API Key',
};

export const CLOUD_PROVIDERS: Record<string, CloudProviderDefinition> = {
  [sonioxProvider.id]: sonioxProvider,
};

export const getCloudProviderByModel = (modelName: string): CloudProviderDefinition | undefined =>
  CLOUD_PROVIDERS[modelName];
