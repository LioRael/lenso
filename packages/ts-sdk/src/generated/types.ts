/* eslint-disable */
// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.

export type CreateUserRequest = {
  display_name?: string | null;
  email: string;
};

export type CreateUserResponse = {
  created_at: string;
  display_name?: string | null;
  email: string;
  id: string;
};

export type ErrorResponse = {
  error: ErrorBody;
};

export type ErrorBody = {
  code: string;
  correlation_id?: string | null;
  details: Array<ValidationErrorDetail>;
  message: string;
  request_id?: string | null;
};

export type ValidationErrorDetail = {
  field?: string | null;
  reason: string;
};

