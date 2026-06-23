export interface AccelerationStatus {
  mode: string;
  effective_backend: string;
  gpu_available: boolean | null;
  message: string;
  diagnostic_code: string;
  recommended_action: string;
  last_error?: string | null;
}
