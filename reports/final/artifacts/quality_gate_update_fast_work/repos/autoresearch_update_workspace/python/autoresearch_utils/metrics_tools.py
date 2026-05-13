from collections import Counter


def claim_status_counts(claims):
    counts = Counter(claim["status"] for claim in claims)
    for status in ["supported", "partially_supported", "contradicted", "not_tested", "unknown"]:
        counts.setdefault(status, 0)
    return dict(counts)

