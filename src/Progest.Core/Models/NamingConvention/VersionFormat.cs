namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Defines version format options for suffixes.
/// </summary>
public enum VersionFormat
{
    /// <summary>
    /// Semantic versioning: v1.0.0
    /// </summary>
    Semantic,

    /// <summary>
    /// Sequential numbering: 001, 002, etc.
    /// </summary>
    Sequential,

    /// <summary>
    /// Simple version: v1.0
    /// </summary>
    Simple
}
