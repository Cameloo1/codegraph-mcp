def refresh_session_token(session, now):
    session["last_seen"] = now
    session["version"] = session.get("version", 0) + 1
    return session


def is_session_active(session):
    return not session.get("revoked", False)
