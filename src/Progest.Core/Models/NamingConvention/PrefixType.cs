namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Defines the type of prefix to apply to a naming convention.
/// </summary>
public enum PrefixType
{
    /// <summary>
    /// No prefix
    /// </summary>
    None,

    /// <summary>
    /// Date-based prefix (e.g., 20250215)
    /// </summary>
    Date,

    /// <summary>
    /// Fixed string prefix
    /// </summary>
    Fixed
}
