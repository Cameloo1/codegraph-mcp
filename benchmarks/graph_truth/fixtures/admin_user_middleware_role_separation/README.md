# Admin User Middleware Role Separation

Trap: requireAdmin and requireUser coexist and both call checkRole. Role strings must stay attached to the correct route/middleware path.
