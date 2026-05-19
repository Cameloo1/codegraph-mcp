export const QUASAR_NETTLE_FUSE = "quasar-nettle-fuse";

export interface RareFuseConfig {
  quasarNettleFuseEnabled: boolean;
}

export function calibrateQuasarNettleFuse(config: RareFuseConfig): boolean {
  return config.quasarNettleFuseEnabled;
}

export function unrelatedCommonHandler(enabled: boolean): boolean {
  return enabled;
}

