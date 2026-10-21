const {
  normalizePreferences,
  shouldSendNotification,
  filterNotificationsForUser,
} = require('../notifications/notificationPreferences');

describe('notificationPreferences', () => {
  describe('normalizePreferences', () => {
    test('returns defaults when fields are omitted', () => {
      const result = normalizePreferences({ userId: 'user-1' });

      expect(result).toEqual({
        userId: 'user-1',
        email: true,
        push: true,
        sms: false,
        categories: [],
      });
    });

    test('returns defaults when preferences object is empty', () => {
      const result = normalizePreferences({});

      expect(result.email).toBe(true);
      expect(result.push).toBe(true);
      expect(result.sms).toBe(false);
      expect(result.categories).toEqual([]);
    });

    test('respects per-channel overrides', () => {
      const result = normalizePreferences({
        userId: 'user-2',
        email: false,
        push: false,
        sms: true,
      });

      expect(result.email).toBe(false);
      expect(result.push).toBe(false);
      expect(result.sms).toBe(true);
    });

    test('preserves provided categories', () => {
      const result = normalizePreferences({
        userId: 'user-3',
        categories: ['alerts', 'news'],
      });

      expect(result.categories).toEqual(['alerts', 'news']);
    });

    test('handles non-object entries gracefully', () => {
      expect(() => normalizePreferences(null)).not.toThrow();
      expect(() => normalizePreferences(undefined)).not.toThrow();
      expect(() => normalizePreferences('string')).not.toThrow();
      expect(() => normalizePreferences(42)).not.toThrow();
    });

    test('handles missing userId', () => {
      const result = normalizePreferences({ email: false });

      expect(result.userId).toBeUndefined();
      expect(result.email).toBe(false);
      expect(result.push).toBe(true);
      expect(result.sms).toBe(false);
    });

    test('throws TypeError for non-array input', () => {
      expect(() => normalizePreferences([])).toThrow(TypeError);
    });

    test('handles invalid preference values by falling back to defaults', () => {
      const result = normalizePreferences({
        userId: 'user-4',
        email: 'not-a-boolean',
        push: 123,
        sms: null,
        categories: 'not-an-array',
      });

      expect(result.email).toBe(true);
      expect(result.push).toBe(true);
      expect(result.sms).toBe(false);
      expect(result.categories).toEqual([]);
    });
  });

  describe('shouldSendNotification', () => {
    const prefs = normalizePreferences({
      userId: 'user-5',
      email: false,
      push: true,
      sms: false,
      categories: ['alerts'],
    });

    test('returns false for disabled channels', () => {
      expect(shouldSendNotification(prefs, { channel: 'email' })).toBe(false);
      expect(shouldSendNotification(prefs, { channel: 'sms' })).toBe(false);
    });

    test('returns true for enabled channels', () => {
      expect(shouldSendNotification(prefs, { channel: 'push' })).toBe(true);
    });

    test('returns false for unknown channels', () => {
      expect(shouldSendNotification(prefs, { channel: 'carrier-pigeon' })).toBe(false);
    });

    test('returns false when category is not allowed', () => {
      expect(
        shouldSendNotification(prefs, { channel: 'push', category: 'news' })
      ).toBe(false);
    });

    test('returns true when category is allowed', () => {
      expect(
        shouldSendNotification(prefs, { channel: 'push', category: 'alerts' })
      ).toBe(true);
    });

    test('returns true for enabled channel when no category is specified', () => {
      expect(shouldSendNotification(prefs, { channel: 'push' })).toBe(true);
    });
  });

  describe('filterNotificationsForUser', () => {
    const prefs = normalizePreferences({
      userId: 'user-6',
      email: true,
      push: false,
      sms: false,
      categories: ['alerts'],
    });

    const notifications = [
      { id: 1, channel: 'email', category: 'alerts' },
      { id: 2, channel: 'push', category: 'alerts' },
      { id: 3, channel: 'email', category: 'news' },
      { id: 4, channel: 'sms', category: 'alerts' },
      { id: 5, channel: 'email' },
    ];

    test('filters out notifications for disabled channels', () => {
      const result = filterNotificationsForUser(notifications, prefs);
      const ids = result.map((n) => n.id);

      expect(ids).not.toContain(2);
      expect(ids).not.toContain(4);
    });

    test('filters out notifications for disallowed categories', () => {
      const result = filterNotificationsForUser(notifications, prefs);
      const ids = result.map((n) => n.id);

      expect(ids).not.toContain(3);
    });

    test('keeps notifications matching enabled channel and allowed category', () => {
      const result = filterNotificationsForUser(notifications, prefs);
      const ids = result.map((n) => n.id);

      expect(ids).toContain(1);
    });

    test('keeps notifications with enabled channel and no category', () => {
      const result = filterNotificationsForUser(notifications, prefs);
      const ids = result.map((n) => n.id);

      expect(ids).toContain(5);
    });

    test('returns empty array when no notifications match', () => {
      const result = filterNotificationsForUser(
        [{ id: 1, channel: 'sms', category: 'alerts' }],
        prefs
      );

      expect(result).toEqual([]);
    });

    test('returns empty array for empty input', () => {
      expect(filterNotificationsForUser([], prefs)).toEqual([]);
    });
  });
});