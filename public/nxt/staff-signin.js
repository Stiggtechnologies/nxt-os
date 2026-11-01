/* Staff sign-in steps. A staff session token is used exactly like the workspace key. */
(() => {
  'use strict';
  const next = (r) => r.mfa_enrollment_required ? 'enroll' : r.must_change_password ? 'password' : 'done';
  globalThis.NXTStaffSignin = { next };
})();
