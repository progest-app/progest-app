using System.Text;
using System.Text.RegularExpressions;

namespace Progest.Core.Extensions;

/// <summary>
/// Provides extension methods for string case conversions.
/// </summary>
public static class StringExtensions
{
    /// <summary>
    /// Converts a string to snake_case.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>The snake_case string.</returns>
    public static string ToSnakeCase(this string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var words = SplitIntoWords(input);
        return string.Join("_", words.Select(w => w.ToLowerInvariant()));
    }

    /// <summary>
    /// Converts a string to camelCase.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>The camelCase string.</returns>
    public static string ToCamelCase(this string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var words = SplitIntoWords(input);
        if (words.Length == 0)
        {
            return string.Empty;
        }

        var result = new StringBuilder();
        result.Append(words[0].ToLowerInvariant());

        for (int i = 1; i < words.Length; i++)
        {
            var word = words[i];
            if (word.Length > 0)
            {
                result.Append(char.ToUpperInvariant(word[0]));
                if (word.Length > 1)
                {
                    result.Append(word.Substring(1).ToLowerInvariant());
                }
            }
        }

        return result.ToString();
    }

    /// <summary>
    /// Converts a string to PascalCase.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>The PascalCase string.</returns>
    public static string ToPascalCase(this string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var words = SplitIntoWords(input);
        if (words.Length == 0)
        {
            return string.Empty;
        }

        var result = new StringBuilder();
        foreach (var word in words)
        {
            if (word.Length > 0)
            {
                result.Append(char.ToUpperInvariant(word[0]));
                if (word.Length > 1)
                {
                    result.Append(word.Substring(1).ToLowerInvariant());
                }
            }
        }

        return result.ToString();
    }

    /// <summary>
    /// Converts a string to kebab-case.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>The kebab-case string.</returns>
    public static string ToKebabCase(this string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var words = SplitIntoWords(input);
        return string.Join("-", words.Select(w => w.ToLowerInvariant()));
    }

    /// <summary>
    /// Converts a string to Title Case.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>The Title Case string.</returns>
    public static string ToTitleCase(this string input)
    {
        if (string.IsNullOrEmpty(input))
        {
            return input ?? string.Empty;
        }

        var words = SplitIntoWords(input);
        if (words.Length == 0)
        {
            return string.Empty;
        }

        var result = new StringBuilder();
        for (int i = 0; i < words.Length; i++)
        {
            var word = words[i];
            if (word.Length > 0)
            {
                result.Append(char.ToUpperInvariant(word[0]));
                if (word.Length > 1)
                {
                    result.Append(word.Substring(1).ToLowerInvariant());
                }

                if (i < words.Length - 1)
                {
                    result.Append(' ');
                }
            }
        }

        return result.ToString();
    }

    /// <summary>
    /// Splits a string into words, handling various delimiters and case patterns.
    /// </summary>
    /// <param name="input">The input string.</param>
    /// <returns>An array of words.</returns>
    private static string[] SplitIntoWords(string input)
    {
        // Replace delimiters (spaces, hyphens, underscores) with spaces
        var normalized = Regex.Replace(input, @"[_\-\s]+", " ");

        // Insert space before uppercase letters that follow lowercase letters or numbers
        normalized = Regex.Replace(normalized, @"([a-z0-9])([A-Z])", "$1 $2");

        // Insert space before uppercase letters that are followed by lowercase letters (for acronyms)
        normalized = Regex.Replace(normalized, @"([A-Z]+)([A-Z][a-z])", "$1 $2");

        // Split by spaces and filter out empty entries
        var words = normalized.Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);

        return words;
    }
}
