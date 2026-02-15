namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Defines the type of naming convention to apply.
/// </summary>
public enum ConventionType
{
    /// <summary>
    /// No specific convention
    /// </summary>
    None,

    /// <summary>
    /// snake_case - all lowercase with underscores
    /// </summary>
    SnakeCase,

    /// <summary>
    /// camelCase - first word lowercase, rest capitalized
    /// </summary>
    CamelCase,

    /// <summary>
    /// PascalCase - all words capitalized
    /// </summary>
    PascalCase,

    /// <summary>
    /// kebab-case - all lowercase with hyphens
    /// </summary>
    KebabCase,

    /// <summary>
    /// Title Case - first letter of each word capitalized with spaces
    /// </summary>
    TitleCase
}
