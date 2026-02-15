using Progest.Core.Extensions;

namespace Progest.Core.Models.NamingConvention;

/// <summary>
/// Represents a naming convention aggregate root that combines prefix, suffix, and case conversion rules.
/// </summary>
public class NamingConvention
{
    /// <summary>
    /// Gets the name of the naming convention.
    /// </summary>
    public string Name { get; }

    /// <summary>
    /// Gets the type of case conversion to apply.
    /// </summary>
    public ConventionType Type { get; private set; }

    /// <summary>
    /// Gets the prefix rule to apply.
    /// </summary>
    public PrefixRule? Prefix { get; private set; }

    /// <summary>
    /// Gets the suffix rule to apply.
    /// </summary>
    public SuffixRule? Suffix { get; private set; }

    /// <summary>
    /// Gets the sequential numbering rule for batch operations.
    /// </summary>
    public SequentialNumberingRule? SequentialNumbering { get; private set; }

    /// <summary>
    /// Initializes a new instance of the NamingConvention class with a name.
    /// </summary>
    /// <param name="name">The name of the naming convention.</param>
    public NamingConvention(string name)
        : this(name, ConventionType.None, null, null, null)
    {
    }

    /// <summary>
    /// Initializes a new instance of the NamingConvention class with name and type.
    /// </summary>
    /// <param name="name">The name of the naming convention.</param>
    /// <param name="type">The type of case conversion to apply.</param>
    public NamingConvention(string name, ConventionType type)
        : this(name, type, null, null, null)
    {
    }

    /// <summary>
    /// Initializes a new instance of the NamingConvention class with name, type, and prefix.
    /// </summary>
    /// <param name="name">The name of the naming convention.</param>
    /// <param name="type">The type of case conversion to apply.</param>
    /// <param name="prefix">The prefix rule to apply.</param>
    public NamingConvention(string name, ConventionType type, PrefixRule? prefix)
        : this(name, type, prefix, null, null)
    {
    }

    /// <summary>
    /// Initializes a new instance of the NamingConvention class with name, type, prefix, and suffix.
    /// </summary>
    /// <param name="name">The name of the naming convention.</param>
    /// <param name="type">The type of case conversion to apply.</param>
    /// <param name="prefix">The prefix rule to apply.</param>
    /// <param name="suffix">The suffix rule to apply.</param>
    public NamingConvention(string name, ConventionType type, PrefixRule? prefix, SuffixRule? suffix)
        : this(name, type, prefix, suffix, null)
    {
    }

    /// <summary>
    /// Initializes a new instance of the NamingConvention class with all parameters.
    /// </summary>
    /// <param name="name">The name of the naming convention.</param>
    /// <param name="type">The type of case conversion to apply.</param>
    /// <param name="prefix">The prefix rule to apply.</param>
    /// <param name="suffix">The suffix rule to apply.</param>
    /// <param name="sequentialNumbering">The sequential numbering rule for batch operations.</param>
    public NamingConvention(
        string name,
        ConventionType type,
        PrefixRule? prefix,
        SuffixRule? suffix,
        SequentialNumberingRule? sequentialNumbering)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Name cannot be null or whitespace.", nameof(name));
        }

        Name = name;
        Type = type;
        Prefix = prefix;
        Suffix = suffix;
        SequentialNumbering = sequentialNumbering;
    }

    /// <summary>
    /// Applies the naming convention to a single input string.
    /// </summary>
    /// <param name="input">The input string to transform.</param>
    /// <returns>The transformed string with all rules applied.</returns>
    public string Apply(string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var result = ApplyCaseConversion(input);
        result = ApplyPrefix(result);
        result = ApplySuffix(result);

        return result;
    }

    /// <summary>
    /// Applies the naming convention to a single input string with sequential numbering for batch operations.
    /// </summary>
    /// <param name="input">The input string to transform.</param>
    /// <param name="index">The index in the batch sequence.</param>
    /// <returns>The transformed string with all rules applied including sequential numbering.</returns>
    public string ApplyBatch(string input, int index)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var result = ApplyCaseConversion(input);

        if (SequentialNumbering != null)
        {
            var number = SequentialNumbering.StartNumber + index;
            result = SequentialNumbering.Format(number) + result;
        }

        result = ApplyPrefix(result);
        result = ApplySuffix(result);

        return result;
    }

    private string ApplyCaseConversion(string input)
    {
        return Type switch
        {
            ConventionType.SnakeCase => input.ToSnakeCase(),
            ConventionType.CamelCase => input.ToCamelCase(),
            ConventionType.PascalCase => input.ToPascalCase(),
            ConventionType.KebabCase => input.ToKebabCase(),
            ConventionType.TitleCase => input.ToTitleCase(),
            ConventionType.None => input,
            _ => throw new InvalidOperationException($"Unknown convention type: {Type}")
        };
    }

    private string ApplyPrefix(string input)
    {
        return Prefix?.GeneratePrefix() + input ?? input;
    }

    private string ApplySuffix(string input)
    {
        return input + (Suffix?.GenerateSuffix() ?? string.Empty);
    }
}
