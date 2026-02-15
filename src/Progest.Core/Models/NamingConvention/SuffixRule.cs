namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Represents a suffix rule for naming conventions.
/// </summary>
public class SuffixRule
{
    /// <summary>
    /// Gets the type of suffix.
    /// </summary>
    public SuffixType Type { get; }

    /// <summary>
    /// Gets the fixed string value when Type is Fixed.
    /// </summary>
    public string? FixedValue { get; }

    /// <summary>
    /// Gets the version format when Type is Version.
    /// </summary>
    public VersionFormat? VersionFormat { get; }

    /// <summary>
    /// Gets the major version number.
    /// </summary>
    public int MajorVersion { get; }

    /// <summary>
    /// Gets the minor version number.
    /// </summary>
    public int MinorVersion { get; }

    /// <summary>
    /// Gets the patch version number.
    /// </summary>
    public int PatchVersion { get; }

    /// <summary>
    /// Initializes a new instance of the SuffixRule class with no suffix.
    /// </summary>
    /// <param name="type">The type of suffix (must be None).</param>
    public SuffixRule(SuffixType type)
    {
        if (type != SuffixType.None)
        {
            throw new ArgumentException($"Suffix type {type} requires additional parameters.");
        }

        Type = type;
    }

    /// <summary>
    /// Initializes a new instance of the SuffixRule class with a fixed value.
    /// </summary>
    /// <param name="type">The type of suffix (must be Fixed).</param>
    /// <param name="fixedValue">The fixed string value to use as suffix.</param>
    public SuffixRule(SuffixType type, string fixedValue)
    {
        if (type != SuffixType.Fixed)
        {
            throw new ArgumentException($"Suffix type {type} does not accept a fixed value parameter.");
        }

        if (string.IsNullOrWhiteSpace(fixedValue))
        {
            throw new ArgumentException("Fixed value cannot be null or whitespace.", nameof(fixedValue));
        }

        Type = type;
        FixedValue = fixedValue;
    }

    /// <summary>
    /// Initializes a new instance of the SuffixRule class with semantic versioning.
    /// </summary>
    /// <param name="type">The type of suffix (must be Version).</param>
    /// <param name="versionFormat">The version format (must be Semantic).</param>
    /// <param name="major">Major version number.</param>
    /// <param name="minor">Minor version number.</param>
    /// <param name="patch">Patch version number.</param>
    public SuffixRule(SuffixType type, VersionFormat versionFormat, int major, int minor, int patch)
    {
        if (type != SuffixType.Version)
        {
            throw new ArgumentException($"Suffix type {type} does not accept version parameters.");
        }

        if (versionFormat != Models.NamingConvention.VersionFormat.Semantic)
        {
            throw new ArgumentException($"Version format {versionFormat} requires different constructor overload.");
        }

        if (major < 0 || minor < 0 || patch < 0)
        {
            throw new ArgumentException("Version numbers cannot be negative.");
        }

        Type = type;
        VersionFormat = versionFormat;
        MajorVersion = major;
        MinorVersion = minor;
        PatchVersion = patch;
    }

    /// <summary>
    /// Initializes a new instance of the SuffixRule class with simple versioning.
    /// </summary>
    /// <param name="type">The type of suffix (must be Version).</param>
    /// <param name="versionFormat">The version format (must be Simple).</param>
    /// <param name="major">Major version number.</param>
    /// <param name="minor">Minor version number.</param>
    public SuffixRule(SuffixType type, VersionFormat versionFormat, int major, int minor)
    {
        if (type != SuffixType.Version)
        {
            throw new ArgumentException($"Suffix type {type} does not accept version parameters.");
        }

        if (versionFormat != Models.NamingConvention.VersionFormat.Simple)
        {
            throw new ArgumentException($"Version format {versionFormat} requires different constructor overload.");
        }

        if (major < 0 || minor < 0)
        {
            throw new ArgumentException("Version numbers cannot be negative.");
        }

        Type = type;
        VersionFormat = versionFormat;
        MajorVersion = major;
        MinorVersion = minor;
        PatchVersion = 0;
    }

    /// <summary>
    /// Initializes a new instance of the SuffixRule class with sequential versioning.
    /// </summary>
    /// <param name="type">The type of suffix (must be Version).</param>
    /// <param name="versionFormat">The version format (must be Sequential).</param>
    /// <param name="number">The sequential number.</param>
    public SuffixRule(SuffixType type, VersionFormat versionFormat, int number)
    {
        if (type != SuffixType.Version)
        {
            throw new ArgumentException($"Suffix type {type} does not accept version parameters.");
        }

        if (versionFormat != Models.NamingConvention.VersionFormat.Sequential)
        {
            throw new ArgumentException($"Version format {versionFormat} requires different constructor overload.");
        }

        if (number < 0)
        {
            throw new ArgumentException("Sequential number cannot be negative.");
        }

        Type = type;
        VersionFormat = versionFormat;
        MajorVersion = number;
        MinorVersion = 0;
        PatchVersion = 0;
    }

    /// <summary>
    /// Generates the suffix string based on the rule type.
    /// </summary>
    /// <returns>The generated suffix string.</returns>
    public string GenerateSuffix()
    {
        return Type switch
        {
            SuffixType.None => string.Empty,
            SuffixType.Fixed => FixedValue ?? string.Empty,
            SuffixType.Version => GenerateVersionSuffix(),
            _ => throw new InvalidOperationException($"Unknown suffix type: {Type}")
        };
    }

    private string GenerateVersionSuffix()
    {
        return VersionFormat switch
        {
            Models.NamingConvention.VersionFormat.Semantic => $"v{MajorVersion}.{MinorVersion}.{PatchVersion}",
            Models.NamingConvention.VersionFormat.Simple => $"v{MajorVersion}.{MinorVersion}",
            Models.NamingConvention.VersionFormat.Sequential => MajorVersion.ToString("D3"), // Pad to 3 digits
            _ => throw new InvalidOperationException($"Unknown version format: {VersionFormat}")
        };
    }
}
