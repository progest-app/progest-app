namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Defines the type of suffix to apply to a naming convention.
/// </summary>
public enum SuffixType
{
    /// <summary>
    /// No suffix
    /// </summary>
    None,

    /// <summary>
    /// Version-based suffix (e.g., v1.0, 001)
    /// </summary>
    Version,

    /// <summary>
    /// Fixed string suffix
    /// </summary>
    Fixed
}
